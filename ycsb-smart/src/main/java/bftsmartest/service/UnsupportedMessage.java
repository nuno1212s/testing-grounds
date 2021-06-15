package bftsmartest.service;

import java.nio.ByteBuffer;

public class UnsupportedMessage extends SystemMessage {
    @Override
    public MessageKind getKind() {
        return MessageKind.UNSUPPORTED;
    }
}
