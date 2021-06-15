package bftsmartest.service;

import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.nio.ByteBuffer;

import site.ycsb.ByteArrayByteIterator;
import site.ycsb.ByteIterator;
import site.ycsb.Status;

import bftsmart.tom.ServiceProxy;

public class Client {
    private static final int FIELD_LENGTH = 1024;
    private static final int UPDATE_MAX = 128;

    private ServiceProxy serviceProxy;
    private Update[] updates;
    private int nodeId, updateCount;

    public Client(int nodeId) {
        serviceProxy = new ServiceProxy(nodeId);
        updates = new Update[UPDATE_MAX];
        updateCount = 0;
        this.nodeId = nodeId;
    }

    public static void main(String[] args) {
        AtomicInteger throughput = new AtomicInteger(0);
        Thread testCase = new Thread(() -> {
            Client client = new Client(1001);
            byte ch = 0;
            StringBuilder sb = new StringBuilder();
            final int nodeId = client.nodeId;
            for (;;) {
                sb.append(ch++);
                String s = sb.toString();
                byte[] data = new byte[FIELD_LENGTH];
                Map<String, ByteIterator> row = new HashMap<>();
                row.put(new String(data), new ByteArrayByteIterator(data));
                //System.out.printf("%d: performing update (at %d)\n", nodeId, System.nanoTime());
                client.update(s, s, row, throughput);
                //System.out.printf("%d: update done (at %d)\n", nodeId, System.nanoTime());
                if (ch % 128 == 0) {
                    ch = 0;
                } else {
                    sb.setLength(sb.length() - 1);
                }
            }
        });
        testCase.start();

        try {
            Thread.sleep(30 * 1000);
        } catch (InterruptedException e) {
            System.exit(1);
        }

        int effectiveThroughput = throughput.get();
        System.out.println("Throughput: " + effectiveThroughput);
        System.out.println("Throughput per sec: " + (effectiveThroughput / 30));
    }

    public Status update(String table, String key, Map<String, ByteIterator> values, AtomicInteger throughput) {
        updates[updateCount++] = new Update(table, key, values);

        if (updateCount % UPDATE_MAX == 0) {
            updateCount = 0;
            byte[] request = (new RequestMessage(updates)).serialize().array();
            byte[] reply = serviceProxy.invokeOrdered(request);
            ReplyMessage message = (ReplyMessage)
                SystemMessage.deserializeAs(ReplyMessage.class, ByteBuffer.wrap(reply));
            throughput.getAndAdd(UPDATE_MAX);
            return message.getStatus();
        }

        return Status.OK;
    }

    public Status update(String table, String key, Map<String, ByteIterator> values) {
        updates[updateCount++] = new Update(table, key, values);

        if (updateCount % UPDATE_MAX == 0) {
            updateCount = 0;
            byte[] request = (new RequestMessage(updates)).serialize().array();
            byte[] reply = serviceProxy.invokeOrdered(request);
            ReplyMessage message = (ReplyMessage)
                SystemMessage.deserializeAs(ReplyMessage.class, ByteBuffer.wrap(reply));
            return message.getStatus();
        }

        return Status.OK;
    }
}
