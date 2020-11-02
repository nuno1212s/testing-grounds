package main

import (
    "os"
    "fmt"
    "net"
    "time"
    "sync/atomic"
)

const (
    src = ":50001"
    dst = "127.0.0.1:50001"
)

func main() {
    if len(os.Args) < 2 {
        usage()
    }
    switch os.Args[1] {
    default:
        usage()
    case "server":
        server()
    case "client":
        client()
    }
}

func server() {
    ln, err := net.Listen("tcp", src)
    if err != nil {
        panic(err)
    }
    defer ln.Close()
    ops := testcase(func(ready chan<- uint64, quit *int32) {
        // sync
        conn, err := ln.Accept()
        if err != nil {
            return
        }
        write(conn)
        read(conn)
        ready <- 0
        // start
        var counter uint64
        for atomic.LoadInt32(quit) == 0 {
            conn, err := ln.Accept()
            if err != nil {
                return
            }
            go func(conn net.Conn) {
                write(conn)
                read(conn)
                atomic.AddUint64(&counter, 1)
                conn.Close()
            }(conn)
        }
        ready <- atomic.LoadUint64(&counter)
    })
    opsPerSecond := float64(ops) / 5.0
    fmt.Printf("%f requests per second\n", opsPerSecond)
}

func client() {
    sem := make(chan net.Conn, 100)
    go func() {
        for {
            conn := <-sem
            if conn == nil {
                return
            }
            read(conn)
            write(conn)
            conn.Close()
        }
    }()
    for {
        conn, err := net.Dial("tcp", dst)
        if err != nil {
            close(sem)
            return
        }
        sem <- conn
    }
}

func testcase(job func(ready chan<- uint64, quit *int32)) uint64 {
    ready := make(chan uint64)
    quit := int32(0)
    go job(ready, &quit)

    <-ready
    time.Sleep(5 * time.Second)
    atomic.StoreInt32(&quit, 1)

    return <-ready
}

func write(c net.Conn) {
    var buf [4096]byte
    _, err := c.Write(buf[:])
    if err != nil {
        c.Close()
        panic(err)
    }
}

func read(c net.Conn) {
    var buf [4096]byte
    _, err := c.Read(buf[:])
    if err != nil {
        c.Close()
        panic(err)
    }
}

func usage() {
    panic("server|client")
}
