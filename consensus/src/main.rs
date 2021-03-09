// XXX: for now focus on first come first serve,
// then implement request queues, and work our
// way up from there

use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug)]
struct Config {
    f: u32,
    id: u32,
    addrs: Vec<SocketAddr>,
}

#[derive(Debug)]
enum ErrorKind {
    DisconnectedTx(u32),
    DisconnectedRx(u32),
}

#[derive(Debug)]
enum Message {
    System(SystemMessage),
    ConnectedTx(u32, TcpStream),
    ConnectedRx(u32, TcpStream),
    Error(ErrorKind),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
enum SystemMessage {
    Request(RequestMessage),
    Consensus(ConsensusMessage),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct RequestMessage {
    value: i32,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
struct ConsensusMessage {
    seq: i32,
    from: u32,
    kind: ConsensusMessageKind,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
enum ConsensusMessageKind {
    PrePrepare(i32),
    Prepare,
    Commit,
}

#[derive(Debug, Copy, Clone)]
enum ProtoPhase {
    Init,
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
    value: i32,
    node: Node,
    tbo_pre_prepare: VecDeque<VecDeque<ConsensusMessage>>,
    tbo_prepare: VecDeque<VecDeque<ConsensusMessage>>,
    tbo_commit: VecDeque<VecDeque<ConsensusMessage>>,
    requests: VecDeque<RequestMessage>,
}

#[derive(Debug)]
struct Node {
    id: u32,
    addrs: Vec<SocketAddr>,
    others_tx: HashMap<u32, Arc<Mutex<TcpStream>>>,
    my_tx: MessageChannelTx,
    my_rx: MessageChannelRx,
}

#[derive(Debug)]
enum SendTo {
    Me(MessageChannelTx),
    Others(Arc<Mutex<TcpStream>>),
}

#[derive(Clone, Debug)]
struct MessageChannelTx {
    other: mpsc::Sender<Message>,
    requests: mpsc::Sender<RequestMessage>,
    consensus: mpsc::Sender<ConsensusMessage>,
}

#[derive(Debug)]
struct MessageChannelRx {
    other: mpsc::Receiver<Message>,
    requests: mpsc::Receiver<RequestMessage>,
    consensus: mpsc::Receiver<ConsensusMessage>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("========================");
    // our replica's id
    let id: u32 = std::env::var("ID")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let mut sys = System::boot(
        Config {
            id,
            f: 1,
            addrs: vec![
                "127.0.0.1:10001".parse().unwrap(),
                "127.0.0.1:10002".parse().unwrap(),
                "127.0.0.1:10003".parse().unwrap(),
                "127.0.0.1:10004".parse().unwrap(),
            ],
        }
    ).await?;

    // fake the client requests for now,
    // for testing purposes
    let tx = sys.node.my_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        for i in 1000.. {
            let m = RequestMessage { value: i };
            let m = SystemMessage::Request(m);
            let m = Message::System(m);
            tx.send(m).await.unwrap_or(());
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    sys.replica_loop().await
}

impl System {
    async fn boot(cfg: Config) -> io::Result<Self> {
        if cfg.addrs.len() < (3*cfg.f as usize + 1) {
            let e = io::Error::new(io::ErrorKind::Other, "invalid no. of replicas");
            return Err(e);
        }
        if cfg.id as usize >= cfg.addrs.len() {
            let e = io::Error::new(io::ErrorKind::Other, "invalid node id");
            return Err(e);
        }
        let id = cfg.id;
        let n = cfg.addrs.len() as u32;

        let listener = TcpListener::bind(cfg.addrs[id as usize]).await?;
        let mut others_tx = HashMap::new();

        let (tx, mut rx) = new_message_channel(128);

        // rx side (accept conns from replica)
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let tx = tx_clone;
            loop {
                if let Ok((mut conn, _)) = listener.accept().await {
                    let id = conn.read_u32().await.unwrap();
                    tx.send(Message::ConnectedRx(id, conn)).await.unwrap_or(());
                }
            }
        });

        // tx side (connect to replica)
        for other_id in (0..n).filter(|&x| x != id) {
            let tx = tx.clone();
            let addr = cfg.addrs[other_id as usize];
            tokio::spawn(async move {
                // try 4 times
                for _ in 0..4 {
                    if let Ok(mut conn) = TcpStream::connect(addr).await {
                        conn.write_u32(id).await.unwrap();
                        tx.send(Message::ConnectedTx(other_id, conn)).await.unwrap_or(());
                        return;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                panic!("something went wrong :]")
            });
        }

        for _ in 0..(2 * (n - 1)) {
            let message = rx.recv()
                .await
                .ok_or_else(||
                    io::Error::new(io::ErrorKind::Other, "connection problems!"))?;
            match message {
                Message::ConnectedTx(id, conn) => {
                    others_tx.insert(id, Arc::new(Mutex::new(conn)));
                },
                Message::ConnectedRx(id, mut conn) => {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        loop {
                            let mut size = [0; 4];
                            if let Err(_) = conn.read_exact(&mut size[..]).await {
                                let m = Message::Error(ErrorKind::DisconnectedRx(id));
                                tx.send(m).await.unwrap_or(());
                                return;
                            }
                            let mut buf = vec![0; u32::from_be_bytes(size) as usize];
                            if let Err(_) = conn.read_exact(&mut buf[..]).await {
                                let m = Message::Error(ErrorKind::DisconnectedRx(id));
                                tx.send(m).await.unwrap_or(());
                                return;
                            }
                            if let Ok(m) = bincode::deserialize(&buf) {
                                tx.send(Message::System(m)).await.unwrap_or(());
                            }
                        }
                    });
                },
                m => panic!("{:?}", m),
            }
        }

        let phase = ProtoPhase::Init;
        let node = Node {
            id,
            addrs: cfg.addrs,
            others_tx,
            my_tx: tx,
            my_rx: rx,
        };
        Ok(System {
            n,
            f: cfg.f,
            seq: 0,
            leader: 0,
            value: 0,
            requests: VecDeque::new(),
            tbo_pre_prepare: VecDeque::new(),
            tbo_prepare: VecDeque::new(),
            tbo_commit: VecDeque::new(),
            phase,
            node,
        })
    }

    fn quorum(&self) -> u32 {
        2*self.f + 1
    }

    #[inline]
    async fn replica_loop(&mut self) -> io::Result<()> {
        // TODO:
        //  - handle errors
        let mut get_queue = false;
        loop {
            let message = match self.phase {
                ProtoPhase::End => return Ok(()),
                ProtoPhase::Init => {
                    println!("A");
                    if let Some(request) = self.requests.pop_front() {
                        if self.leader == self.node.id {
                            self.propose_value(request.value);
                        }
                        self.phase = ProtoPhase::PrePreparing;
                    }
                    let message = self.node.receive().await?;
                    message
                },
                ProtoPhase::PrePreparing if get_queue => {
                    println!("B");
                    if let Some(m) = pop_message(&mut self.tbo_pre_prepare) {
                        Message::System(SystemMessage::Consensus(m))
                    } else {
                        get_queue = false;
                        continue;
                    }
                },
                ProtoPhase::Preparing(_) if get_queue => {
                    println!("C");
                    if let Some(m) = pop_message(&mut self.tbo_prepare) {
                        Message::System(SystemMessage::Consensus(m))
                    } else {
                        get_queue = false;
                        continue;
                    }
                },
                ProtoPhase::Commiting(_) if get_queue => {
                    println!("D");
                    if let Some(m) = pop_message(&mut self.tbo_commit) {
                        Message::System(SystemMessage::Consensus(m))
                    } else {
                        get_queue = false;
                        continue;
                    }
                },
                _ => {
                    println!("E");
                    let message = self.node.receive().await?;
                    message
                },
            };
            println!("{:?}", message);
            match message {
                Message::System(message) => {
                    match message {
                        SystemMessage::Request(message) => {
                            self.requests.push_back(message);
                        },
                        SystemMessage::Consensus(message) => {
                            println!("F");
                            let new_phase = self.process_consensus(message);
                            match (self.phase, new_phase) {
                                (ProtoPhase::Executing, ProtoPhase::Init) => {
                                    advance_message_queue(&mut self.tbo_pre_prepare);
                                    advance_message_queue(&mut self.tbo_prepare);
                                    advance_message_queue(&mut self.tbo_commit);
                                    self.seq += 1;
                                },
                                (_, _) => (),
                            };
                            self.phase = new_phase;
                            get_queue = true;
                        },
                        // ....
                    }
                },
                // ....
                m => panic!("{:?}", m),
            };
        }
    }

    #[inline]
    fn propose_value(&self, value: i32) {
        let message = self.new_consensus_msg(ConsensusMessageKind::PrePrepare(value));
        println!("{:?} ~> {:?}", self.phase, message);
        self.node.broadcast(message, 0_u32..self.n);
    }

    #[inline]
    fn process_consensus(&mut self, message: ConsensusMessage) -> ProtoPhase {
        // TODO: make sure a replica doesn't vote twice
        // by keeping track of who voted, and not just
        // the amount of votes received
        match self.phase {
            ProtoPhase::End => self.phase,
            ProtoPhase::Init => {
                match message.kind {
                    ConsensusMessageKind::PrePrepare(_) => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Prepare => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Commit => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return self.phase;
                    },
                }
            },
            ProtoPhase::PrePreparing => {
                self.value = match message.kind {
                    ConsensusMessageKind::PrePrepare(_) if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::PrePrepare(value) => value,
                    ConsensusMessageKind::Prepare => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Commit => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return self.phase;
                    },
                };
                if self.node.id != self.leader {
                    let message = self.new_consensus_msg(ConsensusMessageKind::Prepare);
                    println!("{:?} ~> {:?}", self.phase, message);
                    self.node.broadcast(message, 0_u32..self.n);
                }
                ProtoPhase::Preparing(0)
            },
            ProtoPhase::Preparing(i) => {
                let i = match message.kind {
                    ConsensusMessageKind::PrePrepare(_) => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Prepare if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Prepare => i + 1,
                    ConsensusMessageKind::Commit => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return self.phase;
                    },
                };
                if i == self.quorum() {
                    let message = self.new_consensus_msg(ConsensusMessageKind::Commit);
                    println!("{:?} ~> {:?}", self.phase, message);
                    self.node.broadcast(message, 0_u32..self.n);
                    ProtoPhase::Commiting(0)
                } else {
                    ProtoPhase::Preparing(i)
                }
            },
            ProtoPhase::Commiting(i) => {
                let i = match message.kind {
                    ConsensusMessageKind::PrePrepare(_) => {
                        queue_message(self.seq, &mut self.tbo_pre_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Prepare => {
                        queue_message(self.seq, &mut self.tbo_prepare, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Commit if message.seq != self.seq => {
                        queue_message(self.seq, &mut self.tbo_commit, message);
                        return self.phase;
                    },
                    ConsensusMessageKind::Commit => i + 1,
                };
                if i == self.quorum() {
                    ProtoPhase::Executing
                } else {
                    ProtoPhase::Commiting(i)
                }
            },
            ProtoPhase::Executing => {
                eprintln!("Value executed on r{} -> {}", self.node.id, self.value);
                ProtoPhase::Init
            },
        }
    }

    fn new_consensus_msg(&self, kind: ConsensusMessageKind) -> SystemMessage {
        SystemMessage::Consensus(ConsensusMessage::new(self.node.id, self.seq, kind))
    }
}

impl Node {
    fn send_to(&self, id: u32) -> SendTo {
        if self.id != id {
            let inner = Arc::clone(self.others_tx.get(&id).unwrap());
            SendTo::Others(inner)
        } else {
            let inner = self.my_tx.clone();
            SendTo::Me(inner)
        }
    }

    fn broadcast(&self, m: SystemMessage, targets: impl Iterator<Item = u32>) {
        for id in targets {
            let send_to = self.send_to(id);
            tokio::spawn(async move {
                send_to.value(m).await.unwrap();
            });
        }
    }

    async fn receive(&mut self) -> io::Result<Message> {
        self.my_rx.recv().await
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "receive failed"))
    }
}

impl SendTo {
    async fn value(&self, m: SystemMessage) -> io::Result<()> {
        async fn me(m: SystemMessage, s: &MessageChannelTx) -> io::Result<()> {
            Ok(s.send(Message::System(m)).await.unwrap_or(()))
        }
        async fn others(m: SystemMessage, l: &Mutex<TcpStream>) -> io::Result<()> {
            let buf = bincode::serialize(&m).unwrap();
            let len = (buf.len() as u32).to_be_bytes();
            let mut s = l.lock().await;
            s.write_all(&len).await?;
            s.write_all(&buf).await
        }
        match self {
            SendTo::Me(ref inner) => me(m, inner).await,
            SendTo::Others(ref inner) => others(m, &*inner).await,
        }
    }
}

fn new_message_channel(bound: usize) -> (MessageChannelTx, MessageChannelRx) {
    let (c_tx, c_rx) = mpsc::channel(bound);
    let (r_tx, r_rx) = mpsc::channel(bound);
    let (o_tx, o_rx) = mpsc::channel(bound);
    let tx = MessageChannelTx {
        consensus: c_tx,
        requests: r_tx,
        other: o_tx,
    };
    let rx = MessageChannelRx {
        consensus: c_rx,
        requests: r_rx,
        other: o_rx,
    };
    (tx, rx)
}

impl MessageChannelTx {
    async fn send(&self, message: Message) -> Result<(), Message> {
        match message {
            Message::System(message) => {
                match message {
                    SystemMessage::Request(message) => {
                        self.requests
                            .send(message)
                            .await
                            .map_err(|e| Message::System(SystemMessage::Request(e.0)))
                    },
                    SystemMessage::Consensus(message) => {
                        self.consensus
                            .send(message)
                            .await
                            .map_err(|e| Message::System(SystemMessage::Consensus(e.0)))
                    },
                }
            },
            _ => {
                self.other
                    .send(message)
                    .await
                    .map_err(|e| e.0)
            },
        }
    }
}

impl MessageChannelRx {
    async fn recv(&mut self) -> Option<Message> {
        let message = tokio::select! {
            c = self.consensus.recv() => Message::System(SystemMessage::Consensus(c?)),
            r = self.requests.recv() => Message::System(SystemMessage::Request(r?)),
            o = self.other.recv() => o?,
        };
        Some(message)
    }
}

impl ConsensusMessage {
    fn new(from: u32, seq: i32, kind: ConsensusMessageKind) -> Self {
        Self { seq, from, kind }
    }
}

fn pop_message(tbo: &mut VecDeque<VecDeque<ConsensusMessage>>) -> Option<ConsensusMessage> {
    if tbo.is_empty() {
        None
    } else {
        tbo[0].pop_front()
    }
}

fn queue_message(curr_seq: i32, tbo: &mut VecDeque<VecDeque<ConsensusMessage>>, m: ConsensusMessage) {
    let index = m.seq - curr_seq;
    if index < 0 {
        // drop old messages
        return;
    }
    let index = index as usize;
    if index >= tbo.len() {
        let len = index - tbo.len() + 1;
        tbo.extend(std::iter::repeat_with(VecDeque::new).take(len));
    }
    tbo[index].push_back(m);
}

fn advance_message_queue(tbo: &mut VecDeque<VecDeque<ConsensusMessage>>) {
    tbo.pop_front();
}
