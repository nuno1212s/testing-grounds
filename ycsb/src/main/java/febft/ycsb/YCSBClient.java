package febft.ycsb;

import java.util.Map;
import java.io.IOException;
import java.security.Security;

import org.bouncycastle.jce.provider.BouncyCastleProvider;

import febft.ycsb.Node;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

import site.ycsb.ByteIterator;
import site.ycsb.DB;

public class YCSBClient extends DB {
    private Node node;

    public void init() {
        if (args.length < 2) {
            usage();
        }

        try {
            String sep = System.getProperty("file.separator");
            BasicConfigurator.configure(new FileAppender(new SimpleLayout(), String.format("log%s%d", sep, Node.ID), false));
        } catch (IOException e) {
            System.err.println("Failed to create log file.");
            System.exit(1);
        }

        Security.addProvider(new BouncyCastleProvider());
    }

    @Override
    public int delete(String arg0, String arg1) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int scan(String arg0, String arg1, int arg2, Set<String> arg3,
                    Vector<Map<String, ByteIterator>> arg4) {
        throw new UnsupportedOperationException();
    }
}
