package bftsmartest;

import java.io.IOException;
import java.security.Security;

import org.bouncycastle.jce.provider.BouncyCastleProvider;

import org.apache.log4j.FileAppender;
import org.apache.log4j.SimpleLayout;
import org.apache.log4j.BasicConfigurator;

import bftsmartest.service.Replica;

public class YCSBServer {
    public static void main(String[] args) {
        if (args.length < 1) {
            usage();
        }

        final int nodeId = Integer.parseInt(args[0]);

        try {
            BasicConfigurator.configure(new FileAppender(new SimpleLayout(), String.format("log/%d", nodeId), false));
        } catch (IOException e) {
            System.err.println("Failed to create log file.");
            System.exit(1);
        }

        Security.addProvider(new BouncyCastleProvider());
        Replica.main(nodeId);
    }

    public static void usage() {
        System.err.println("Usage: YCSBServer <id>");
        System.exit(1);
    }
}
