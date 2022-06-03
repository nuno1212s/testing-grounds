package microbenchmarks_async.benchmarks;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.util.concurrent.atomic.AtomicBoolean;

public class OSStatistics extends Thread {

    private final int node_id;

    private final AtomicBoolean cancelled;

    private final String command;

    public OSStatistics(int node_id, String command) {
        this.node_id = node_id;
        this.command = command;
        this.cancelled = new AtomicBoolean(false);
    }

    public void cancel() {
        this.cancelled.set(true);
    }

    @Override
    public void run() {

        System.out.println("Starting OS Monitoring");

        while (!cancelled.get()) {
            try {

                Process proc = Runtime.getRuntime().exec(String.format(command, node_id));

                proc.waitFor();

                if (proc.exitValue() != 0) {
                    System.out.println("Failed to execute OS statistics script. " + proc.exitValue() + " ");

                    BufferedReader error_reader = new BufferedReader(new InputStreamReader(proc.getErrorStream())),
                            input_reader = new BufferedReader(new InputStreamReader(proc.getInputStream()));

                    String read;

                    while ((read = error_reader.readLine()) != null) {
                        System.out.println(read);
                    }

                    while ((read = input_reader.readLine()) != null) {
                        System.out.println(read);
                    }

                    break;
                }

                final BufferedReader reader = new BufferedReader(new InputStreamReader(proc.getInputStream()));

                String s;

                while ((s = reader.readLine()) != null) {
                    System.out.println(s);
                }

            } catch (IOException | InterruptedException e) {
                e.printStackTrace();
            }

            try {
                Thread.sleep(250);
            } catch (InterruptedException e) {
                e.printStackTrace();
            }
        }

    }
}
