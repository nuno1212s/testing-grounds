package febft.ycsb;

import java.util.Map;
import java.nio.ByteOrder;
import java.nio.ByteBuffer;

import febft.ycsb.MessageKind;
import febft.ycsb.SystemMessage;
import febft.ycsb.capnp.Messages.System;
import febft.ycsb.capnp.Messages.Request;
import febft.ycsb.capnp.Messages.Value;

import site.ycsb.ByteIterator;

import org.capnproto.StructList;
import org.capnproto.MessageBuilder;

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
    protected SystemMessage deserialize(ByteBuffer buf) {
        return null
    }
}
