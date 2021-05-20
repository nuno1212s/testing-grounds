package febft.ycsb;

import io.github.rctcwyvrn.blake3.Blake3;

import java.nio.ByteBuffer;

public class Header {
    private static final int CURRENT_VERSION = 0;
    private static final int SIGNATURE_LEN = 64;
    private static final int DIGEST_LEN = 32;

    public static final int LENGTH =
        4 + 4 + 4 + 4 + 8 + 8 + DIGEST_LEN + SIGNATURE_LEN;

    private int version;
    private int from;
    private int to;
    private long nonce;
    private long length;
    private byte[] digest;
    private byte[] signature;

    private Header() {
        // nothing
    }

    public Header(int from, int to, long nonce, byte[] payload) {
        this(from, to, nonce, payload,
            (payload != null && payload.length > 0) ? payload.length : 0);
    }

    public Header(int from, int to, long nonce, byte[] payload, long length) {
        this.version = CURRENT_VERSION;
        this.from = from;
        this.to = to;
        this.nonce = nonce;
        this.length = length;
        this.signature = new byte[SIGNATURE_LEN];
        if (this.length > 0) {
            Blake3 ctx = Blake3.newInstance();
            ctx.update(payload);
            this.digest = ctx.digest();
        } else {
            this.digest = new byte[DIGEST_LEN];
        }
    }

    // buf needs to be in little endian mode
    public void serializeInto(ByteBuffer buf) {
        buf.putInt(0);
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
        header.digest = new byte[DIGEST_LEN];
        header.signature = new byte[SIGNATURE_LEN];

        int _align = buf.getInt();
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

    public String toString() {
        return (new StringBuilder())
            .append("Header { version: ")
            .append(version)
            .append(", from: ")
            .append(from)
            .append(", to: ")
            .append(to)
            .append(", nonce: ")
            .append(nonce)
            .append(", .. }")
            .toString();
    }
}
