package febft.ycsb;

import java.util.List;
import java.util.ArrayList;
import java.util.Collection;
import java.util.concurrent.Future;
import java.util.concurrent.Callable;
import java.util.concurrent.Executors;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.ExecutionException;

public class Pool {
    private static final int F = 1;
    private static final int QUORUM = 2*F + 1;

    private static ExecutorService INSTANCE = Executors.newCachedThreadPool();
    //private static ExecutorService INSTANCE = Executors.newWorkStealingPool();

    // TODO: wait only for a quorum
    public static <T> T call(Collection<? extends Callable<T>> callables) {
        T result = null;
        try {
            List<Future<T>> futures = new ArrayList<>();
            for (Callable<T> callable : callables) {
                futures.add(INSTANCE.submit(callable));
            }
            int resolved = 0;
            while (resolved < QUORUM) {
                for (int i = 0; i < futures.size(); i++) {
                    Future<T> fut = futures.get(i);
                    if (fut.isDone()) {
                        result = fut.get();
                        futures.remove(i);
                    }
                }
                Thread.sleep(100);
            }
        } catch (ExecutionException e) {
            System.err.printf("FATAL: Execution exception: %s", e);
            System.exit(1);
        } catch (InterruptedException e) {
            return null;
        } finally {
            return result;
        }
    }
}
