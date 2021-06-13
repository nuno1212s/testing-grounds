package bftsmartest;

import java.util.*;

import site.ycsb.RandomByteIterator;
import site.ycsb.ByteIterator;
import site.ycsb.Status;

import bftsmart.tom.ServiceProxy;

public class Client {
    private ServiceProxy serviceProxy;

    public Client(int nodeId) {
        serviceProxy = new ServiceProxy(nodeId);
    }

    public static void main(String[] args) {
        // use this size for each map value
        final int fieldLength = 1024;
    }

    public Status update(String table, String key, Map<String, ByteIterator> values) {
        byte[] request = null;
        byte[] reply = serviceProxy.invokeOrdered(request);
    }
}
