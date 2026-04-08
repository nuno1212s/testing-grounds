package bftsmartest.service;

import java.io.*;
import java.util.*;
import java.nio.ByteBuffer;

import bftsmart.tom.MessageContext;
import bftsmart.tom.ServiceReplica;
import bftsmart.tom.server.defaultservices.DefaultSingleRecoverable;

import site.ycsb.Status;
import site.ycsb.ByteIterator;

public class Replica extends DefaultSingleRecoverable {
    private Map<String, Map<String, Map<String, byte[]>>> databases;

    public Replica(int nodeId) {
        databases = new HashMap<>();
        new ServiceReplica(nodeId, this, this);
    }

    public static void main(int nodeId) {
        new Replica(nodeId);
    }

    @Override
    public byte[] appExecuteOrdered(byte[] command, MessageContext msgCtx) {
        RequestMessage message = (RequestMessage)
            SystemMessage.deserializeAs(RequestMessage.class, ByteBuffer.wrap(command));
        Update[] updates = message.getUpdates();

        for (Update update : updates) {
            Map<String, Map<String, byte[]>> rows = databases.computeIfAbsent(update.getTable(), (k) -> new HashMap<>());
            Map<String, byte[]> row = new HashMap<>();

            for (Map.Entry<String, ByteIterator> pair : update.getValues().entrySet()) {
                long n = pair.getValue().bytesLeft();
                row.put(pair.getKey(), new byte[(int)(n < 0 ? -n : n)]);
            }

            rows.put(update.getKey(), row);
        }

        return (new ReplyMessage(Status.OK, new byte[]{0})).serialize().array();
    }

    @Override
    public byte[] appExecuteUnordered(byte[] command, MessageContext msgCtx) {
        throw new UnsupportedOperationException();
    }

    @SuppressWarnings("unchecked")
    @Override
    public void installSnapshot(byte[] state) {
        try {
            ByteArrayInputStream bis = new ByteArrayInputStream(state);
            ObjectInput in = new ObjectInputStream(bis);
            databases = (Map<String, Map<String, Map<String, byte[]>>>)in.readObject();
            in.close();
            bis.close();
        } catch (IOException | ClassNotFoundException e) {
            System.err.println("[ERROR] Error deserializing state: "
                    + e.getMessage());
        }
    }

    @Override
    public byte[] getSnapshot() {
        try {
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            ObjectOutput out = new ObjectOutputStream(bos);
            out.writeObject(databases);
            out.flush();
            bos.flush();
            out.close();
            bos.close();
            return bos.toByteArray();
        } catch (IOException ioe) {
            System.err.println("[ERROR] Error serializing state: "
                    + ioe.getMessage());
            return "ERROR".getBytes();
        }
    }
}
