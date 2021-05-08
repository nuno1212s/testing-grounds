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

import org.capnproto.Serialize;
import org.capnproto.StructList;
import org.capnproto.MessageBuilder;

public class RequestMessage extends SystemMessage {
    private String table, key;
    private Map<String, ByteIterator> values;

    public RequestMessage(String table, String key, Map<String, ByteIterator> values) {
        this.table = table;
        this.key = key;
        this.values = values;
    }

    @Override
    public MessageKind getKind() {
        return MessageKind.REQUEST;
    }

    @Override
    public ByteBuffer serialize() {
        MessageBuilder message = new MessageBuilder();
        System.Builder systemMessage = message.initRoot(System.factory);
        Request.Builder request = systemMessage.initRequest();

        request.setTable(table);
        request.setKey(key);

        int i = 0;
        StructList.Builder<Value.Builder> reqVals = request.initValues(values.size());

        for (Map.Entry<String, ByteIterator> pair : values.entrySet()) {
            Value.Builder entry = reqVals.get(i);

            entry.setKey(pair.getKey());
            entry.setValue(pair.getValue().toArray());

            ++i;
        }

        long size = Serialize.computeSerializedSizeInWords(message);
        ByteBuffer output = ByteBuffer.allocate((int)size);

        ByteBuffer[] segments = message.getSegmentsForOutput();
        int tableSize = (segments.length + 2) & (~1);

        ByteBuffer table = ByteBuffer.allocate(4 * tableSize);
        table.order(ByteOrder.LITTLE_ENDIAN);

        table.putInt(0, segments.length - 1);

        for (i = 0; i < segments.length; ++i) {
            table.putInt(4 * (i + 1), segments[i].limit() / 8);
        }

        output.put(table);
        for (ByteBuffer buffer : segments) {
            output.put(buffer);
        }

        return output;
    }
}
