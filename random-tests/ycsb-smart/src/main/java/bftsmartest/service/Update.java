package bftsmartest.service;

import java.util.Map;

import site.ycsb.ByteIterator;

public class Update {
    private String table, key;
    private Map<String, ByteIterator> values;

    public Update(String table, String key, Map<String, ByteIterator> values) {
        this.table = table;
        this.key = key;
        this.values = values;
    }

    public String getTable() {
        return table;
    }

    public String getKey() {
        return key;
    }

    public Map<String, ByteIterator> getValues() {
        return values;
    }
}
