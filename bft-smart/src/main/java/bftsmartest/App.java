package bftsmartest;

import java.io.IOException;

import bftsmartest.map.MapServer;
import bftsmartest.map.MapInteractiveClient;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

public class App {
    public static void main(String[] args) {
        if (args.length < 2) {
            usage();
        }

        final int nodeId = Integer.parseInt(args[1]);

        try {
            BasicConfigurator.configure(new FileAppender(new SimpleLayout(), String.format("log/%d", nodeId), false));
        } catch (IOException e) {
            System.err.println("Failed to create log file.");
            System.exit(1);
        }

        switch (args[0]) {
        case "client":
            MapInteractiveClient.main(nodeId);
            break;
        case "server":
            MapServer.main(nodeId);
            break;
        default:
            usage();
        }
    }

    public static void usage() {
        System.err.println("Usage: App <client|server> <id>");
        System.exit(1);
    }
}
