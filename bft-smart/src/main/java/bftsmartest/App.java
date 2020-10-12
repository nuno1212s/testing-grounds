package bftsmartest;

import bftsmartest.map.MapServer;
import bftsmartest.map.MapInteractiveClient;

public class App {
    public static void main(String[] args) {
        if (args.length < 2) {
            usage();
        }

        final int nodeId = Integer.parseInt(args[1]);

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
