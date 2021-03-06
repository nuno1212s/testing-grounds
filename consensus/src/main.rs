// XXX: for now focus on first come first serve,
// then implement request queues, and work our
// way up from there

use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct Message {
    seq: i32,
    from: u32,
    kind: MessageKind,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
enum MessageKind {
    PrePrepare(i32),
    Prepare,
    Commit,
}

#[derive(Debug)]
enum ProtoPhase {
    Boot,
    PrePreparing,
    Preparing(u32),
    Commiting(u32),
    Executing,
    End,
}

#[derive(Debug)]
struct System {
    phase: ProtoPhase,
    seq: i32,
    leader: u32,
    n: u32,
    f: u32,
    node: Node,
    value: i32,
    tbo_pre_prepare: Vec<Vec<Message>>,
    tbo_prepare: Vec<Vec<Message>>,
    tbo_commit: Vec<Vec<Message>>,
}

#[derive(Debug)]
enum CommSide {
    Tx((u32, TcpStream)),
    Rx((u32, TcpStream)),
}

#[derive(Debug)]
struct Node {
    id: u32,
    others_tx: HashMap<u32, Arc<TcpStream>>,
    others_rx: HashMap<u32, Arc<TcpStream>>,
    my_tx: Arc<mpsc::Sender<Message>>,
    my_rx: Arc<Mutex<mpsc::Receiver<Message>>>,
}

#[derive(Debug)]
enum SendTo {
    Me(Arc<mpsc::Sender<Message>>),
    Others(Arc<TcpStream>),
}

#[derive(Debug)]
enum RecvFrom {
    Me(Arc<Mutex<mpsc::Receiver<Message>>>),
    Others(Arc<TcpStream>),
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // our replica's id
    let id: u32 = std::env::var("ID")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut sys = System::boot(id).await?;

    let values = std::env::args()
        .nth(1)
        .unwrap();

    sys.consensus_loop(&values).await
}

impl System {
    async fn boot(id: u32) -> io::Result<Self> {
        // assume we're using 4 nodes -> f = 1;
        // assume leader id = 0; others = 1, 2, 3;
        let listener = TcpListener::bind(format!("127.0.0.1:{}", 10000 + id)).await?;
        let mut others_tx = HashMap::new();
        let mut others_rx = HashMap::new();

        let (tx, mut rx) = mpsc::channel(8);

        // rx side (accept conns from replica)
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let tx = tx_clone;
            loop {
                if let Ok((mut conn, _)) = listener.accept().await {
                    let id = conn.read_u32().await.unwrap();
                    tx.send(CommSide::Rx((id, conn))).await.unwrap_or(());
                }
            }
        });

        // tx side (connect to replica)
        for other_id in (0_u32..4_u32).filter(|&x| x != id) {
            let tx = tx.clone();
            tokio::spawn(async move {
                let addr = format!("127.0.0.1:{}", 10000 + other_id);
                // try 4 times
                for _ in 0..4 {
                    if let Ok(mut conn) = TcpStream::connect(&addr).await {
                        conn.write_u32(id).await.unwrap();
                        tx.send(CommSide::Tx((other_id, conn))).await.unwrap();
                        return;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                panic!("something went wrong :]")
            });
        }

        for _ in 0..6 {
            let received = rx.recv()
                .await
                .ok_or_else(||
                    io::Error::new(io::ErrorKind::Other, "connection problems!"))?;
            match received {
                CommSide::Tx((id, conn)) => others_tx.insert(id, Arc::new(conn)),
                CommSide::Rx((id, conn)) => others_rx.insert(id, Arc::new(conn)),
            };
        }

        let phase = ProtoPhase::Boot;
        let c = mpsc::channel(8);
        let (my_tx, my_rx) = (Arc::new(c.0), Arc::new(Mutex::new(c.1)));
        let node = Node { id, others_tx, others_rx, my_tx, my_rx };
        Ok(System {
            n: 4,
            f: 1,
            seq: 0,
            leader: 0,
            value: 0,
            tbo_pre_prepare: Vec::new(),
            tbo_prepare: Vec::new(),
            tbo_commit: Vec::new(),
            phase,
            node,
        })
    }

    fn quorum(&self) -> u32 {
        2*self.f + 1
    }

    #[inline]
    async fn consensus_loop(&mut self, values: &str) -> io::Result<()> {
        let mut input = values.split_whitespace();
        let mut buf = String::new();
        while !self.consensus_step(&mut input, &mut buf).await? {
            // nothing
        }
        Ok(())
    }

    #[inline]
    async fn propose_value(&self, value: i32) -> io::Result<()> {
        match self.phase {
            ProtoPhase::PrePreparing if self.node.id == self.leader => {
                let message = self.new_msg(MessageKind::PrePrepare(value));
                self.node.broadcast(message, 0_u32..self.n);
                Ok(())
            },
            ProtoPhase::PrePreparing => Ok(()),
            _ => Err(io::Error::new(io::ErrorKind::Other, "tried proposing on invalid phase"))
        }
    }

    #[inline]
    async fn process_message(&mut self, message: Message) -> io::Result<ProtoPhase> {
        match self.phase {
            ProtoPhase::Boot | ProtoPhase::End => Ok(self.phase),
            ProtoPhase::PrePreparing => {
                self.value = match message.kind {
                    MessageKind::PrePrepare(value) if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::PrePrepare(value) => value,
                    MessageKind::Prepare => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Commit => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return Ok(self.phase);
                    },
                };
                Ok(ProtoPhase::Preparing(0))
            },
            ProtoPhase::Preparing(i) => {
                let i = match message.kind {
                    MessageKind::PrePrepare(_) => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Prepare if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Prepare => i + 1,
                    MessageKind::Commit => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return Ok(self.phase);
                    },
                };
                if i == self.quorum() {
                    if self.node.id != self.leader {
                        self.node.broadcast(self.new_msg(MessageKind::Prepare), 0_u32..self.n);
                    }
                    Ok(ProtoPhase::Commiting(0))
                } else {
                    Ok(ProtoPhase::Preparing(i))
                }
            },
            ProtoPhase::Commiting(i) => {
                let i = match message.kind {
                    MessageKind::PrePrepare(_) => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Prepare => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Commit if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return Ok(self.phase);
                    },
                    MessageKind::Commit => i + 1,
                };
                if i == self.quorum() {
                    self.node.broadcast(self.new_msg(MessageKind::Commit), 0_u32..self.n);
                    Ok(ProtoPhase::Executing)
                } else {
                    Ok(ProtoPhase::Commiting(i))
                }
            },
            ProtoPhase::Executing => {
                eprintln!("Value executed on r{} -> {}", self.node.id, self.value);
                Ok(ProtoPhase::PrePreparing)
            },
        }
    }

    fn new_msg(&self, kind: MessageKind) -> Message {
        Message::new(self.node.id, self.seq, kind)
    }
}

impl Node {
    fn send_to(&self, id: u32) -> SendTo {
        if self.id != id {
            let inner = Arc::clone(self.others_tx.get(&id).unwrap());
            SendTo::Others(inner)
        } else {
            let inner = Arc::clone(&self.my_tx);
            SendTo::Me(inner)
        }
    }

    fn recv_from(&self, id: u32) -> RecvFrom {
        if self.id != id {
            let inner = Arc::clone(self.others_rx.get(&id).unwrap());
            RecvFrom::Others(inner)
        } else {
            let inner = Arc::clone(&self.my_rx);
            RecvFrom::Me(inner)
        }
    }

    fn broadcast(&self, m: Message, targets: impl Iterator<Item = u32>) {
        for id in targets {
            let send_to = self.send_to(id);
            tokio::spawn(async move {
                send_to.value(m).await.unwrap();
            });
        }
    }

    fn receive(&self, targets: impl Iterator<Item = u32>) -> mpsc::Receiver<Message> {
        let (tx, rx) = mpsc::channel(8);
        for id in targets {
            let recv_from = self.recv_from(id);
            let tx = tx.clone();
            tokio::spawn(async move {
                let message = recv_from.value().await.unwrap();
                tx.send(message).await.unwrap_or(());
            });
        }
        rx
    }
}

impl SendTo {
    async fn value(&self, m: Message) -> io::Result<()> {
        async fn me(m: Message, s: &mpsc::Sender<Message>) -> io::Result<()> {
            Ok(s.send(m).await.unwrap_or(()))
        }
        async fn write(s: &TcpStream, buf: &[u8]) -> io::Result<()> {
            let mut i = 0;
            loop {
                s.writable().await?;
                match s.try_write(&buf[i..]) {
                    Ok(n) => {
                        if n == buf.len() - i {
                            return Ok(());
                        }
                        i += n;
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        async fn others(m: Message, s: &TcpStream) -> io::Result<()> {
            let buf = bincode::serialize(&m).unwrap();
            let len = (buf.len() as u32).to_be_bytes();
            write(s, &len).await?;
            write(s, &buf).await
        }
        match self {
            SendTo::Me(ref inner) => me(m, &*inner).await,
            SendTo::Others(ref inner) => others(m, &*inner).await,
        }
    }
}

impl RecvFrom {
    async fn value(&self) -> io::Result<Message> {
        async fn me(s: &Mutex<mpsc::Receiver<Message>>) -> io::Result<Message> {
            Ok(s.lock().await.recv().await.unwrap())
        }
        async fn read(s: &TcpStream, buf: &mut [u8]) -> io::Result<()> {
            let mut i = 0;
            loop {
                s.readable().await?;
                match s.try_read(&mut buf[i..]) {
                    Ok(n) => {
                        if n == buf.len() - i {
                            return Ok(());
                        }
                        i += n;
                    },
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
        async fn others(s: &TcpStream) -> io::Result<Message> {
            let mut size = [0; 4];
            read(s, &mut size[..]).await?;
            let mut buf = vec![0; u32::from_be_bytes(size) as usize];
            read(s, &mut buf[..]).await?;
            Ok(bincode::deserialize(&buf).unwrap())
        }
        match self {
            RecvFrom::Me(ref inner) => me(&*inner).await,
            RecvFrom::Others(ref inner) => others(&*inner).await,
        }
    }
}

impl Message {
    fn new(from: u32, seq: i32, kind: MessageKind) -> Self {
        Self { seq, from, kind }
    }
}

fn pop_message(tbo: &mut Vec<Vec<Message>>) -> Option<Message> {
    if tbo.is_empty() {
        None
    } else {
        tbo[0].pop()
    }
}

fn queue_message(curr_seq: i32, tbo: &mut Vec<Vec<Message>>, m: Message) {
    let index = m.seq - curr_seq;
    if index < 0 {
        // drop old messages
        return;
    }
    let index = index as usize;
    if index >= tbo.len() {
        let len = index - tbo.len() + 1;
        tbo.extend(std::iter::repeat_with(Vec::new).take(len));
    }
    tbo[index].push(m);
}

fn advance_message_queue(tbo: &mut Vec<Vec<Message>>) {
    if !tbo.is_empty() {
        tbo.remove(0);
    }
}
