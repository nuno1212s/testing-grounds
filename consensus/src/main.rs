// XXX: for now focus on first come first serve,
// then implement request queues, and work our
// way up from there

use tokio::io;
use tokio::fs::File;
use tokio::io::BufReader;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;

// the three protocol phases
const PHASE_PRE_PREPARE: [u8; 4] = *b"PPPR";
const PHASE_PREPARE: [u8; 4] = *b"PRPR";
const PHASE_COMMIT: [u8; 4] = *b"CMIT";

#[derive(Debug)]
enum ProtoPhase {
    Init,
    PrePreparing,
    Preparing,
    Commiting,
    Executing,
}

#[derive(Debug)]
struct System {
    view: u32,
    phase: ProtoPhase,
    node: Node,
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
//    my_tx: Arc<mpsc::Sender<i32>>,
//    my_rx: Arc<mpsc::Receiver<i32>>,
}

#[derive(Debug)]
struct SendTo {
    inner: Arc<TcpStream>,
}

#[derive(Debug)]
struct RecvFrom {
    inner: Arc<TcpStream>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // our replica's id
    let id: u32 = std::env::var("ID")
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let mut sys = System::boot(id).await?;

    sys.consensus_loop().await
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
                    tx.send(CommSide::Rx((id, conn))).await.unwrap();
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
                    std::io::Error::new(std::io::ErrorKind::Other, "connection problems!"))?;
            match received {
                CommSide::Tx((id, conn)) => others_tx.insert(id, Arc::new(conn)),
                CommSide::Rx((id, conn)) => others_rx.insert(id, Arc::new(conn)),
            };
        }

        let phase = ProtoPhase::Init;
        let node = Node { id, others_tx, others_rx };
        Ok(System { view: 0, phase, node })
    }

    #[inline]
    async fn consensus_loop(&mut self) -> io::Result<()> {
        let mut buf = String::new();
        let mut input = BufReader::new(File::open("/tmp/consensus/input").await?);
        while !self.consensus_step(&mut input, &mut buf).await? {
            // nothing
        }
        Ok(())
    }

    #[inline]
    async fn consensus_step(&mut self, mut input: impl Unpin + AsyncBufRead, buf: &mut String) -> io::Result<bool> {
        match self.phase {
            // leader
            ProtoPhase::Init if self.node.id == self.view => {
                println!("< INIT        r{} >", self.node.id);
                let n = input.read_line(buf).await?;
                if n == 0 {
                    return Ok(true);
                }
                self.phase = ProtoPhase::PrePreparing;
            },
            // backup
            ProtoPhase::Init => {
                println!("< INIT        r{} >", self.node.id);
                self.phase = ProtoPhase::PrePreparing;
            },
            // leader
            ProtoPhase::PrePreparing if self.node.id == self.view => {
                println!("< PRE-PREPARE r{} >", self.node.id);
                let value: i32 = buf.parse().unwrap_or(0);
                for id in (0_u32..4_u32).filter(|&x| x != self.node.id) {
                    let send_to_replica = self.node.send_to(id);
                    let buf = buf.clone();
                    tokio::spawn(async move {
                        let value = value.to_be_bytes();
                        send_to_replica.value(&PHASE_PRE_PREPARE[..]).await.unwrap_or(());
                        send_to_replica.value(value.as_ref()).await.unwrap_or(());
                    });
                }
                self.phase = ProtoPhase::Preparing;
            },
            // backup
            ProtoPhase::PrePreparing => {
                println!("< PRE-PREPARE r{} >", self.node.id);
                // XXX XXX XXX XXX XXX XXX XXX XXX
                // XXX XXX XXX XXX XXX XXX XXX XXX
                // XXX XXX XXX XXX XXX XXX XXX XXX
                // XXX XXX XXX XXX XXX XXX XXX XXX
                // This should be in the PREPARE phase
                let (tx, mut rx) = mpsc::channel(8);
                let mut counter = 0;
                for id in (0_u32..4_u32).filter(|&x| x != self.node.id) {
                    let recv_from_replica = self.node.recv_from(id);
                    let buf = buf.clone();
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let mut buf = [0; 8];
                        recv_from_replica.value(&mut buf[..]).await.unwrap();
                        if &buf[..4] != &PHASE_PRE_PREPARE[..] {
                            panic!("INVALID PHASE");
                        }
                        let value = i32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
                        tx.send(value).await.unwrap();
                    });
                }
                let mut received = 0;
                loop {
                    let value = rx.recv().await.unwrap();
                    match counter {
                        0 => {
                            received = value;
                            counter += 1;
                        },
                        // 2f+1 = 2*1 + 1 = 3
                        _ if counter == 3 => {
                            break;
                        },
                        _ => {
                            if value != received {
                                panic!("DIFFERENT");
                            }
                            counter += 1;
                        },
                    }
                }
                self.phase = ProtoPhase::Preparing;
            },
            ProtoPhase::Preparing => {
                println!("< PREPARE     r{} >", self.node.id);
                self.phase = ProtoPhase::Commiting;
            },
            ProtoPhase::Commiting => {
                println!("< COMMIT      r{} >", self.node.id);
                self.phase = ProtoPhase::Executing;
            },
            ProtoPhase::Executing => {
                print!("< EXECUTE     r{} > {}", self.node.id, buf);
                buf.clear();
                self.phase = ProtoPhase::Init;
            },
        }
        Ok(false)
    }
}

impl Node {
    fn send_to(&self, id: u32) -> SendTo {
        let inner = Arc::clone(self.others_tx.get(&id).unwrap());
        SendTo { inner }
    }

    fn recv_from(&self, id: u32) -> RecvFrom {
        let inner = Arc::clone(self.others_rx.get(&id).unwrap());
        RecvFrom { inner }
    }
}

impl SendTo {
    async fn value(&self, buf: &[u8]) -> io::Result<()> {
        let mut i = 0;
        loop {
            self.inner.writable().await?;
            match self.inner.try_write(&buf[i..]) {
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
}

impl RecvFrom {
    async fn value(&self, buf: &mut [u8]) -> io::Result<()> {
        let mut i = 0;
        loop {
            self.inner.readable().await?;
            match self.inner.try_read(&mut buf[i..]) {
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
}
