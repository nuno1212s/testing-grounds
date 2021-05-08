package febft.ycsb;

import java.nio.ByteBuffer;

import febft.ycsb.MessageKind;

public abstract class SystemMessage {
    private static SystemMessage INSTANCE = (SystemMessage)new Object();

    public ByteBuffer serialize() {
        throw new UnsupportedOperationException();
    }

    public SystemMessage deserializeFrom(ByteBuffer buf) {
        throw new UnsupportedOperationException();
    }

    public static SystemMessage deserialize(ByteBuffer buf) {
        return INSTANCE.deserializeFrom(buf);
    }

    public abstract MessageKind getKind();
}
