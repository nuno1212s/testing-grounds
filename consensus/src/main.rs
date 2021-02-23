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

    if sys.node.id == 0 {
        sys.leader_loop().await
    } else {
        //sys.backup_loop().await
        let mut buf = [0; 4096];
        let n = sys.node.recv_from(0).value(&mut buf[..]).await?;
        let s = std::str::from_utf8(&buf[..n]).unwrap();
        print!("{}", s);
        Ok(())
    }
}

impl System {
    async fn boot(id: u32) -> io::Result<Self> {
        // assume we're using 4 nodes -> f = 1;
        // assume leader id = 0; others = 1, 2, 3;
        let listener = TcpListener::bind(format!("127.0.0.1:1000{}", id)).await?;
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
                let addr = format!("127.0.0.1:1000{}", other_id);
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
        Ok(System { phase, node })
    }

    #[inline]
    async fn leader_loop(&mut self) -> io::Result<()> {
        let mut buf = String::new();
        let mut input = BufReader::new(File::open("/tmp/consensus/input").await?);
        while !self.leader_step(&mut input, &mut buf).await? {
            // nothing
        }
        Ok(())
    }

    #[inline]
    async fn leader_step(&mut self, mut input: impl Unpin + AsyncBufRead, buf: &mut String) -> io::Result<bool> {
        match self.phase {
            ProtoPhase::Init => {
                println!("< INIT        r{} >", self.node.id);
                let n = input.read_line(buf).await?;
                if n == 0 {
                    return Ok(true);
                }
                self.phase = ProtoPhase::PrePreparing;
            },
            ProtoPhase::PrePreparing => {
                println!("< PRE-PREPARE r{} >", self.node.id);
                for id in (0_u32..4_u32).filter(|&x| x != self.node.id) {
                    let send_to_replica = self.node.send_to(id);
                    send_to_replica.value(buf.as_ref()).await?; // TODO: spawn task
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

    async fn backup_loop(&mut self) -> io::Result<()> {
        Ok(())
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
        loop {
            self.inner.writable().await?;
            match self.inner.try_write(buf) {
                Ok(n) if n != buf.len() => return Err(io::Error::new(io::ErrorKind::Other, "Short write")),
                Ok(_) => return Ok(()),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl RecvFrom {
    async fn value(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            self.inner.readable().await?;
            match self.inner.try_read(buf) {
                Ok(n) => return Ok(n),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
}
