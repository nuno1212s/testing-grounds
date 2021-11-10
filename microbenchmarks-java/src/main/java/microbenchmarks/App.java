package microbenchmarks;

import java.util.UUID;
import java.util.Arrays;
import java.io.IOException;
import java.security.Security;

import org.bouncycastle.jce.provider.BouncyCastleProvider;

import microbenchmarks.impl.ThroughputLatencyClient;
import microbenchmarks.impl.ThroughputLatencyServer;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

public class App {
    public static void main(String[] args) throws IOException {
        if (args.length < 1) {
            usage();
        }

        final String logPath = "log/" + UUID.randomUUID() + ".log";
        BasicConfigurator.configure(new FileAppender(new SimpleLayout(), logPath, false));
        Security.addProvider(new BouncyCastleProvider());

        String[] newArgs = null;

        switch (args[0]) {
        case "client":
            newArgs = Arrays.copyOfRange(args, 1, args.length);
            ThroughputLatencyClient.main(newArgs);
            break;
        case "server":
            newArgs = Arrays.copyOfRange(args, 1, args.length);
            ThroughputLatencyServer.main(newArgs);
            break;
        default:
            usage();
        }
    }

    public static void usage() {
        System.err.println("Usage: App <client|server> ...");
        System.exit(1);
    }
}
