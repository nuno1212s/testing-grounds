package febft.ycsb;

import java.nio.ByteBuffer;

import febft.ycsb.MessageKind;
import febft.ycsb.SystemMessage;

public class UnsupportedMessage extends SystemMessage {
    @Override
    public MessageKind getKind() {
        return MessageKind.UNSUPPORTED;
    }
}
