package febft.ycsb;

import java.util.Map;
import java.nio.ByteOrder;
import java.nio.ByteBuffer;

import febft.ycsb.Update;
import febft.ycsb.MessageKind;
import febft.ycsb.SystemMessage;
import febft.ycsb.capnp.Messages.System;
import febft.ycsb.capnp.Messages.Request;
import febft.ycsb.capnp.Messages.Value;
import febft.ycsb.capnp.Messages.System;
import febft.ycsb.capnp.Messages;

import site.ycsb.ByteIterator;

import org.capnproto.StructList;
import org.capnproto.MessageBuilder;

public class RequestMessage extends SystemMessage {
    private Update[] updates;

    public RequestMessage(Update... updates) {
        this.updates = updates;
    }

    @Override
    public MessageKind getKind() {
        return MessageKind.REQUEST;
    }

    @Override
    public ByteBuffer serialize() {
        MessageBuilder message = new MessageBuilder();
        System.Builder systemMessage = message.initRoot(System.factory);
        Messages.Update.Builder updateRequest = systemMessage.initRequest();
        StructList.Builder<Request.Builder> updateReqs = updateRequest.initRequests(updates.length);

        for (int k = 0; k < updates.length; k++) {
            Request.Builder request = updateReqs.get(k);

            request.setTable(updates[k].getTable());
            request.setKey(updates[k].getKey());

            int i = 0;
            Map<String, ByteIterator> values = updates[k].getValues();
            StructList.Builder<Value.Builder> reqVals = request.initValues(values.size());

            for (Map.Entry<String, ByteIterator> pair : values.entrySet()) {
                Value.Builder entry = reqVals.get(i);

                entry.setKey(pair.getKey());
                entry.setValue(pair.getValue().toArray());

                ++i;
            }
        }

        ByteBuffer output = ByteBuffer.allocate(2 * 1024 * 1024);

        ByteBuffer[] segments = message.getSegmentsForOutput();
        int tableSize = (segments.length + 2) & (~1);

        ByteBuffer table = ByteBuffer.allocate(4 * tableSize);
        table.order(ByteOrder.LITTLE_ENDIAN);

        table.putInt(0, segments.length - 1);

        for (int i = 0; i < segments.length; ++i) {
            table.putInt(4 * (i + 1), segments[i].limit() / 8);
        }

        output.put(table);
        for (ByteBuffer buffer : segments) {
            output.put(buffer);
        }

        return output;
    }
}
