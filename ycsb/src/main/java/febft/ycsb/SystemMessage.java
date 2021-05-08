package febft.ycsb;

import java.nio.ByteBuffer;

import febft.ycsb.MessageKind;

public abstract class SystemMessage {
    private static SystemMessage INSTANCE = (SystemMessage)new Object();

    public ByteBuffer serialize() {
        throw new UnsupportedOperationException();
    }

    protected SystemMessage deserialize(ByteBuffer buf) {
        throw new UnsupportedOperationException();
    }

    public static <T extends SystemMessage> SystemMessage deserializeAs(Class<T> kls, ByteBuffer buf) {
        return kls.cast(INSTANCE).deserialize(buf);
    }

    public abstract MessageKind getKind();
}
