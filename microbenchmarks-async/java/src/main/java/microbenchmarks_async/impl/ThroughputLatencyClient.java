package microbenchmarks_async.impl;

import bftsmart.communication.client.ReplyListener;
import bftsmart.tom.AsynchServiceProxy;
import bftsmart.tom.RequestContext;
import bftsmart.tom.core.messages.TOMMessage;
import bftsmart.tom.core.messages.TOMMessageType;
import bftsmart.tom.util.Storage;
import bftsmart.tom.util.TOMUtil;
import microbenchmarks_async.benchmarks.OSStatistics;

import java.io.FileWriter;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.security.*;
import java.security.spec.EncodedKeySpec;
import java.security.spec.InvalidKeySpecException;
import java.security.spec.PKCS8EncodedKeySpec;
import java.util.Collection;
import java.util.LinkedList;
import java.util.Random;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;

public class ThroughputLatencyClient {

    public static String privKey = "MD4CAQAwEAYHKoZIzj0CAQYFK4EEAAoEJzAlAgEBBCBnhIob4JXH+WpaNiL72BlbtUMAIBQoM852d+tKFBb7fg==";
    public static String pubKey = "MFYwEAYHKoZIzj0CAQYFK4EEAAoDQgAEavNEKGRcmB7u49alxowlwCi1s24ANOpOQ9UiFBxgqnO/RfOl3BJm0qE2IJgCnvL7XUetwj5C/8MnMWi9ux2aeQ==";

    public static int initId = 0;

    static LinkedBlockingQueue<String> latencies;

    static Thread writerThread;

    static AtomicBoolean writerThreadFlag;

    public static void main(String[] args) {
        if (args.length < 9) {
            System.out.println("Usage: ... ThroughputLatencyClient <init client id> <nr clients> <nr ops> <rq size> <interval (ms)> <concurrent_rqs> <read only?> <verbose?> <nosig | default | ecdsa>");

            System.exit(2);
        }

        System.out.println("Starting clients...");

        initId = Integer.parseInt(args[0]);
        latencies = new LinkedBlockingQueue<>();
        writerThreadFlag = new AtomicBoolean(false);

        writerThread = new Thread(() -> {
            FileWriter f = null;

            try {
                f = new FileWriter("./latencies_" + initId + ".txt");

                while (!writerThreadFlag.get()) {
                    f.write(latencies.take());

                    Thread.sleep(1000);
                }

                f.write(latencies.take());
            } catch (IOException | InterruptedException e) {
                e.printStackTrace();
            } finally {
                try {
                    f.close();
                } catch (IOException e) {
                    e.printStackTrace();
                }
            }
        });

        int client_count = Integer.parseInt(args[1]);
        int nrOps = Integer.parseInt(args[2]);
        int requestSize = Integer.parseInt(args[3]);
        int interval = Integer.parseInt(args[4]);
        int concurrentRqs = Integer.parseInt(args[5]);
        boolean readOnly = Boolean.parseBoolean(args[6]);
        boolean verbose = Boolean.parseBoolean(args[7]);
        String sign = args[8];

        String path = null;

        if (args.length >= 10) {
            path = args[8];
        }

        int s = 0;
        if (!sign.equalsIgnoreCase("nosig")) s++;
        if (sign.equalsIgnoreCase("ecdsa")) s++;

        if (s == 2 && Security.getProvider("SunEC") == null) {

            System.out.println("Option 'ecdsa' requires SunEC provider to be available.");
            System.exit(0);
        }

        Client[] clients = new Client[client_count * nrOps];

        int id = 0;

        for (int cli = 0; cli < client_count; cli++) {
            try {
                Thread.sleep(10);
            } catch (InterruptedException ex) {

                ex.printStackTrace();
            }

            System.out.println("Launching client " + (initId + id));

            clients[id] = new ThroughputLatencyClient.Client(initId + id, nrOps, requestSize, interval,
                    concurrentRqs, readOnly, verbose, s);

            id++;
        }

        if (path != null) {
            OSStatistics statistics = new OSStatistics(initId, path);

            statistics.start();

            Runtime.getRuntime().addShutdownHook(new Thread(statistics::cancel));
        } else {
            System.out.println("Could not start OS Statistics, no path was provided");
        }

        ExecutorService execs = Executors.newFixedThreadPool(clients.length);

        Collection<Future<?>> futures = new LinkedList<>();

        for (Client client : clients) {
            futures.add(execs.submit(client));
        }
        // wait for tasks completion
        for (Future<?> currTask : futures) {
            try {
                currTask.get();
            } catch (InterruptedException | ExecutionException ex) {
                ex.printStackTrace();
            }
        }

        execs.shutdown();

        try {
            writerThreadFlag.set(true);
            writerThread.join();
        } catch (InterruptedException e) {
            // ignore
        }

        System.out.println("All clients done.");
    }


    static class Client extends Thread {

        int id;
        int numberOfOps;
        int requestSize;
        int interval;

        int concurrentRqs;
        boolean readOnly;
        boolean verbose;
        AsynchServiceProxy proxy;
        byte[] request;
        int rampup = 1000;

        public Client(int id, int numberOfOps, int requestSize, int interval, int concurrent_rqs, boolean readOnly, boolean verbose, int sign) {
            super("Client " + id);

            this.id = id;
            this.numberOfOps = numberOfOps;
            this.requestSize = requestSize;
            this.interval = interval;
            this.concurrentRqs = concurrent_rqs;
            this.readOnly = readOnly;
            this.verbose = verbose;
            this.proxy = new AsynchServiceProxy(id);
            this.request = new byte[this.requestSize];

            Random rand = new Random(System.nanoTime() + this.id);
            rand.nextBytes(request);

            byte[] signature = new byte[0];
            Signature eng;

            try {

                if (sign > 0) {

                    if (sign == 1) {
                        eng = TOMUtil.getSigEngine();
                        eng.initSign(proxy.getViewManager().getStaticConf().getPrivateKey());
                    } else {

                        eng = Signature.getInstance("SHA256withECDSA", "BC");

                        //KeyFactory kf = KeyFactory.getInstance("EC", "BC");
                        //Base64.Decoder b64 = Base64.getDecoder();
                        //PKCS8EncodedKeySpec spec = new PKCS8EncodedKeySpec(b64.decode(ThroughputLatencyClient.privKey));
                        //eng.initSign(kf.generatePrivate(spec));
                        KeyFactory keyFactory = KeyFactory.getInstance("EC");
                        EncodedKeySpec privateKeySpec = new PKCS8EncodedKeySpec(org.apache.commons.codec.binary.Base64.decodeBase64(privKey));
                        PrivateKey privateKey = keyFactory.generatePrivate(privateKeySpec);
                        eng.initSign(privateKey);

                    }
                    eng.update(request);
                    signature = eng.sign();
                }

                ByteBuffer buffer = ByteBuffer.allocate(request.length + signature.length + (Integer.BYTES * 2));
                buffer.putInt(request.length);
                buffer.put(request);
                buffer.putInt(signature.length);
                buffer.put(signature);
                this.request = buffer.array();


            } catch (NoSuchAlgorithmException | SignatureException | NoSuchProviderException | InvalidKeyException |
                     InvalidKeySpecException ex) {
                ex.printStackTrace();
                System.exit(0);
            }

        }

        public void run() {

            System.out.println("Warm up...");

            int req = 0;

            Semaphore semaphore = new Semaphore(concurrentRqs);

            for (int i = 0; i < numberOfOps / 2; i++, req++) {
                try {
                    //Attempt to acquire the semaphore.
                    //This should only be possible if there are no more than concurrentRqs currently awaiting response
                    semaphore.acquire();
                } catch (InterruptedException e) {
                    throw new RuntimeException(e);
                }

                if (verbose) System.out.print("Sending req " + req + "...");

                long last_send_instant = System.nanoTime();

                ReplyListener replyListener = new ReplyListener() {
                    @Override
                    public void reset() {
                        //Do nothing
                    }

                    @Override
                    public void replyReceived(RequestContext context, TOMMessage reply) {
                        long latency = System.nanoTime() - last_send_instant;

                        try {
                            if (reply != null) latencies.put(id + "\t" + System.currentTimeMillis() + "\t" + latency + "\n");
                        } catch (InterruptedException ex) {
                            ex.printStackTrace();
                        }

                        //Release another request so the client can go back to making more requests
                        semaphore.release();
                    }
                };

                TOMMessageType type;

                if (readOnly) {
                    type = TOMMessageType.UNORDERED_REQUEST;
                } else {
                    type = TOMMessageType.ORDERED_REQUEST;
                }

                proxy.invokeAsynchRequest(request, replyListener, type);

                if (verbose) System.out.println(" sent!");

                if (verbose && (req % 1000 == 0)) System.out.println(this.id + " // " + req + " operations sent!");

                try {

                    //sleeps interval ms before sending next request
                    if (interval > 0) {
                        Thread.sleep(interval);
                    } else if (this.rampup > 0) {
                        Thread.sleep(this.rampup);
                    }

                    this.rampup -= 100;

                } catch (InterruptedException ex) {
                    ex.printStackTrace();
                }
            }

            Storage st = new Storage(numberOfOps / 2);

            System.out.println("Executing experiment for " + numberOfOps / 2 + " ops");

            for (int i = 0; i < numberOfOps / 2; i++, req++) {

                try {
                    semaphore.acquire();
                } catch (InterruptedException e) {
                    throw new RuntimeException(e);
                }

                if (verbose) System.out.print(this.id + " // Sending req " + req + "...");

                long last_send_instant = System.nanoTime();

                ReplyListener replyListener = new ReplyListener() {
                    @Override
                    public void reset() {
                        //Do nothing
                    }

                    @Override
                    public void replyReceived(RequestContext context, TOMMessage reply) {
                        long latency = System.nanoTime() - last_send_instant;

                        try {
                            if (reply != null) latencies.put(id + "\t" + System.currentTimeMillis() + "\t" + latency + "\n");
                        } catch (InterruptedException ex) {
                            ex.printStackTrace();
                        }

                        st.store(latency);

                        //Release another request so the client can make it again
                        semaphore.release();
                    }
                };

                TOMMessageType type;

                if (readOnly) {
                    type = TOMMessageType.UNORDERED_REQUEST;
                } else {
                    type = TOMMessageType.ORDERED_REQUEST;
                }

                proxy.invokeAsynchRequest(request, replyListener, type);

                try {

                    //sleeps interval ms before sending next request
                    if (interval > 0) {

                        Thread.sleep(interval);
                    } else if (this.rampup > 0) {
                        Thread.sleep(this.rampup);
                    }
                    this.rampup -= 100;

                } catch (InterruptedException ex) {
                    ex.printStackTrace();
                }

                if (verbose && (req % 1000 == 0)) System.out.println(this.id + " // " + req + " operations sent!");
            }

            if (id == initId) {
                System.out.println(this.id + " // Average time for " + numberOfOps / 2 + " executions (-10%) = " + st.getAverage(true) / 1000 + " us ");
                System.out.println(this.id + " // Standard desviation for " + numberOfOps / 2 + " executions (-10%) = " + st.getDP(true) / 1000 + " us ");
                System.out.println(this.id + " // Average time for " + numberOfOps / 2 + " executions (all samples) = " + st.getAverage(false) / 1000 + " us ");
                System.out.println(this.id + " // Standard desviation for " + numberOfOps / 2 + " executions (all samples) = " + st.getDP(false) / 1000 + " us ");
                System.out.println(this.id + " // Maximum time for " + numberOfOps / 2 + " executions (all samples) = " + st.getMax(false) / 1000 + " us ");
            }

            proxy.close();
        }
    }
}
