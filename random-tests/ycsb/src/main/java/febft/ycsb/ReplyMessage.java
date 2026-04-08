package febft.ycsb;

import java.nio.ByteBuffer;
import java.io.IOException;

import febft.ycsb.MessageKind;
import febft.ycsb.SystemMessage;
import febft.ycsb.UnsupportedMessage;
import febft.ycsb.capnp.Messages.System;
import febft.ycsb.capnp.Messages.Reply;

import site.ycsb.Status;

import org.capnproto.Serialize;
import org.capnproto.MessageReader;

public class ReplyMessage extends SystemMessage {
    private Status status;
    private byte[] digest;

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

    public Status getStatus() {
        return status;
    }

    public byte[] getDigest() {
        return digest;
    }
}
