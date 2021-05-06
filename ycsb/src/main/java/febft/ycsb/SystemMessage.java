package febft.ycsb;

import java.nio.ByteBuffer;

import febft.ycsb.MessageKind;

public abstract class SystemMessage {
    public abstract void serializeInto(ByteBuffer buf);

    public abstract SystemMessage deserializeFrom(ByteBuffer buf);

    public abstract MessageKind getKind();
}
