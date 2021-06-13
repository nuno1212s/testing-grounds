package bftsmartest;

import java.util.*;
import java.io.IOException;
import java.security.Security;

import org.bouncycastle.jce.provider.BouncyCastleProvider;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

import site.ycsb.ByteIterator;
import site.ycsb.Status;
import site.ycsb.DB;

import bftsmartest.service.Client;

public class YCSBClient extends DB {
    private Client client;

    @Override
    public void init() {
        final int nodeId = IdCounter.nextId();

        try {
            BasicConfigurator.configure(new FileAppender(new SimpleLayout(), String.format("log/%d", nodeId), false));
        } catch (IOException e) {
            System.err.println("Failed to create log file.");
            System.exit(1);
        }

        Security.addProvider(new BouncyCastleProvider());
        client = new Client(nodeId);
    }

    @Override
    public Status update(String table, String key, Map<String, ByteIterator> values) {
        return client.update(table, key, values);
    }

    @Override
    public Status read(String table, String key, Set<String> fields, Map<String, ByteIterator> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status scan(String table, String startkey, int recordcount, Set<String> fields,
                       Vector<HashMap<String, ByteIterator>> result) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status insert(String table, String key, Map<String, ByteIterator> values) {
        return Status.NOT_IMPLEMENTED;
    }

    @Override
    public Status delete(String table, String key) {
        return Status.NOT_IMPLEMENTED;
    }
}

