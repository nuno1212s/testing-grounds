package bftsmartest.service;

import java.nio.ByteOrder;
import java.nio.ByteBuffer;
import java.io.IOException;

import static bftsmartest.service.capnp.Messages.System;
import static bftsmartest.service.capnp.Messages.Reply;

import site.ycsb.Status;

import org.capnproto.Serialize;
import org.capnproto.MessageReader;
import org.capnproto.MessageBuilder;

public class ReplyMessage extends SystemMessage {
    private Status status;
    private byte[] digest;

    public ReplyMessage() {
        status = Status.ERROR;
        digest = null;
    }

    public ReplyMessage(Status status, byte[] digest) {
        this.status = status;
        this.digest = digest;
    }

    @Override
    public MessageKind getKind() {
        return MessageKind.REPLY;
    }

    @Override
    protected SystemMessage deserialize(ByteBuffer buf) throws IOException {
        MessageReader reader = Serialize.read(buf);
        System.Reader systemMessage = reader.getRoot(System.factory);

        switch (systemMessage.which()) {
        case REQUEST:
        case CONSENSUS:
            return new UnsupportedMessage();
        }

        Reply.Reader reply = systemMessage.getReply();
        boolean ok = reply.getStatus() == 0;
        byte[] digest = reply.getDigest().toArray();
        Status status = ok ? Status.OK : Status.ERROR;

        return new ReplyMessage(status, digest);
    }

    @Override
    public ByteBuffer serialize() {
        MessageBuilder message = new MessageBuilder();
        System.Builder systemMessage = message.initRoot(System.factory);
        Reply.Builder reply = systemMessage.initReply();

        reply.setStatus(status == Status.OK ? 0 : 1);
        reply.setDigest(digest);

        ByteBuffer output = ByteBuffer.allocate(8 * 1024);
        output.clear();

        ByteBuffer[] segments = message.getSegmentsForOutput();
        int tableSize = (segments.length + 2) & (~1);

        ByteBuffer table = ByteBuffer.allocate(4 * tableSize);
        table.order(ByteOrder.LITTLE_ENDIAN);

        table.putInt(0, segments.length - 1);

        for (int i = 0; i < segments.length; ++i) {
            table.putInt(4 * (i + 1), segments[i].limit() / 8);
        }

        output.put(table);
        for (ByteBuffer buffer : segments) {
            output.put(buffer);
        }

        return output;
    }

    public Status getStatus() {
        return status;
    }

    public byte[] getDigest() {
        return digest;
    }
}
