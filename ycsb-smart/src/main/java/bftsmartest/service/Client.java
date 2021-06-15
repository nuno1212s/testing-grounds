package bftsmartest.service;

import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.nio.ByteBuffer;

import site.ycsb.ByteArrayByteIterator;
import site.ycsb.ByteIterator;
import site.ycsb.Status;

import bftsmart.tom.ServiceProxy;

public class Client {
    private static final int FIELD_COUNT = 20000;
    private static final int FIELD_LENGTH = 1024;
    private static final int UPDATE_MAX = 128;

    private ServiceProxy serviceProxy;
    private Update[] updates;
    private int updateCount;

    public Client(int nodeId) {
        serviceProxy = new ServiceProxy(nodeId);
        updates = new Update[UPDATE_MAX];
        updateCount = 0;
    }

    public static void main(String[] args) {
        AtomicInteger throughput = new AtomicInteger(0);
        Thread testCase = new Thread(() -> {
            Client client = new Client(1001);
            for (;;) {
                //client.update(/* ... */);
                throughput.getAndAdd(1);
            }
        });
        testCase.start();

        try {
            Thread.sleep(30);
        } catch (InterruptedException e) {
            System.exit(1);
        }
        System.out.println("Throughput per sec: " + (throughput.get() / 30));
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
