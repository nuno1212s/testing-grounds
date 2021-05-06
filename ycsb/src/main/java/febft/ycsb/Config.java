package febft.ycsb;

import java.util.List;
import java.util.ArrayList;
import java.io.IOException;
import java.io.BufferedReader;
import java.io.FileReader;

public class Config {
    public static List<Entry> parse(String configPath) {
        String configLine;
        List<Entry> config = new ArrayList<>();

        try (BufferedReader reader = new BufferedReader(new FileReader(configPath))) {
            while ((configLine = reader.readLine()) != null) {
                Entry entry = Entry.parse(configLine);
                if (entry != null) {
                    config.add(entry);
                }
            }
        } catch (IOException e) {
            // noop
        } finally {
            return config;
        }
    }

    public static class Entry {
        private int id, portNo;
        private String hostname, ipAddr;

        public static Entry parse(String configLine) {
            final String[] entries = configLine.trim().split("([ ]+)", 4);

            if (entries.length != 4 || entries[0].charAt(0) == '#') {
                return null;
            }

            final int id = Integer.parseInt(entries[0]);
            final String hostname = entries[1];
            final String ipAddr = entries[2];
            final int portNo = Integer.parseInt(entries[3]);

            return new Entry(id, hostname, ipAddr, portNo);
        }

        public Entry(int id, String hostname, String ipAddr, int portNo) {
            this.id = id;
            this.hostname = hostname;
            this.ipAddr = ipAddr;
            this.portNo = portNo;
        }

        public int getId() {
            return id;
        }

        public String getHostname() {
            return hostname;
        }

        public String getIpAddr() {
            return ipAddr;
        }

        public int getPortNo() {
            return portNo;
        }
    }
}
