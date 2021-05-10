package febft.ycsb;

import java.nio.ByteBuffer;
import java.io.IOException;

import febft.ycsb.MessageKind;
import febft.ycsb.SystemMessage;
import febft.ycsb.UnsupportedMessage;
import febft.ycsb.capnp.Messages.System;

import site.ycsb.Status;

import org.capnproto.Serialize;
import org.capnproto.MessageReader;

public class ReplyMessage extends SystemMessage {
    private Status status;

    public ReplyMessage(Status status) {
        this.status = status;
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

        boolean ok = systemMessage.getReply().getStatus() == 0;
        Status status = ok ? Status.OK : Status.ERROR;

        return new ReplyMessage(status);
    }
}
