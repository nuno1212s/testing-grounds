package microbenchmarks.benchmarks;

import java.io.BufferedReader;
import java.io.IOException;
import java.util.concurrent.atomic.AtomicBoolean;

public class OSStatistics extends Thread {

    private final int node_id;

    private final AtomicBoolean cancelled;

    public OSStatistics(int node_id) {
        this.node_id = node_id;
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

                Process proc = Runtime.getRuntime().exec("os_stat " + node_id);

                if (proc.exitValue() != 0) {
                    System.out.println("Failed to execute OS statistics script");
                    break;
                }

                final BufferedReader reader = proc.inputReader();

                String s;

                while ((s = reader.readLine()) != null) {
                    System.out.println(s);
                }

            } catch (IOException e) {
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
