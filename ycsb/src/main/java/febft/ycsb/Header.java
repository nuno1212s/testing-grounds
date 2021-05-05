package febft.ycsb;

import io.lktk.NativeBLAKE3;
import java.nio.ByteBuffer;

public class Header {
    private static final CURRENT_VERSION = 0;

    private int version;
    private int from;
    private int to;
    private long nonce;
    private long length;
    private digest byte[];
    private signature byte[];

    private Header() {
        // nothing
    }

    public Header(int from, int to, long nonce, byte[] payload) {
        NativeBLAKE3 ctx = new NativeBLAKE3();
        ctx.update(payload);

        this.version = CURRENT_VERSION;
        this.from = from;
        this.to = to;
        this.nonce = nonce;
        this.digest = ctx.getOutput();
        this.length = payload.length;
        this.signature = new byte[64];
    }

    // buf needs to be in little endian mode
    public void serializeInto(ByteBuffer buf) {
        buf.putInt(version);
        buf.putInt(from);
        buf.putInt(to);
        buf.putLong(nonce);
        buf.putLong(length);
        buf.put(digest);
        buf.put(signature);
    }

    // buf needs to be in little endian mode
    public static Header deserializeFrom(ByteBuffer buf) {
        Header header = new Header();
        header.digest = new byte[32];
        header.signature = new byte[64];

        header.version = buf.getInt();
        header.from = buf.getInt();
        header.to = buf.getInt();
        header.nonce = buf.getLong();
        header.length = buf.getLong();
        buf.get(header.digest);
        buf.get(header.signature);

        return header;
    }

    public int getVersion() {
        return version;
    }

    public int getFrom() {
        return from;
    }

    public int getTo() {
        return to;
    }

    public long getNonce() {
        return nonce;
    }

    public long getLength() {
        return length;
    }

    public byte[] getDigest() {
        return digest;
    }

    public byte[] getSignature() {
        return signature;
    }
}
