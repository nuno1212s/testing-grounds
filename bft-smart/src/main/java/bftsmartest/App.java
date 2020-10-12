package bftsmart;

import bftsmartest.map.*;

public class App {
    public static void main(String[] args) {
        if (args.length < 3) {
            System.err.printf("Usage: %s <client|server> <id>\n", args[0]);
            System.exit(1);
        }

        final int nodeId = Integer.parseInt(args[2]);

        switch (args[1]) {
        case "client":
            break;
        }
    }
}
