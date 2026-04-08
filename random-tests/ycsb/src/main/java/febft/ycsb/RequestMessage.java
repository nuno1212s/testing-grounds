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
    private int sessionId;
    private int operationId;
    private Update update;

    public RequestMessage(int sessionId, int operationId, Update update) {
        this.sessionId = sessionId;
        this.operationId = operationId;
        this.update = update;
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
        request.setOperationId(operationId);
        request.setSessionId(sessionId);

        Messages.Update.Builder updateRequest = request.initUpdate();
        updateRequest.setTable(update.getTable());
        updateRequest.setKey(update.getKey());

        int i = 0;
        Map<String, ByteIterator> values = update.getValues();
        StructList.Builder<Value.Builder> updateVals = updateRequest.initValues(values.size());

        for (Map.Entry<String, ByteIterator> pair : values.entrySet()) {
            Value.Builder entry = updateVals.get(i);
            long n = pair.getValue().bytesLeft();

            entry.setKey(pair.getKey());
            entry.setValue(new byte[(int)(n < 0 ? -n : n)]);

            ++i;
        }

        ByteBuffer output = ByteBuffer.allocate(20 * 1024 * 1024);
        output.clear();

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
