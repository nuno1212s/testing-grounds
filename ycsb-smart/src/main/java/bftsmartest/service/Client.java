package bftsmartest.service;

import java.util.*;

import site.ycsb.RandomByteIterator;
import site.ycsb.ByteIterator;
import site.ycsb.Status;

import bftsmart.tom.ServiceProxy;

public class Client {
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
    }

    public Status update(String table, String key, Map<String, ByteIterator> values) {
        byte[] request = null;
        byte[] reply = serviceProxy.invokeOrdered(request);
    }
}
