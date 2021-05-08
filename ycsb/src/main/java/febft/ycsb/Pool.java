package febft.ycsb;

import java.util.Collection;
import java.util.concurrent.Future;
import java.util.concurrent.Callable;
import java.util.concurrent.Executors;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.ExecutionException;

public class Pool {
    private static ExecutorService INSTANCE = Executors.newCachedThreadPool();

    // TODO: wait only for a quorum
    public static <T> T call(Collection<? extends Callable<T>> callables) {
        T result = null;
        try {
            for (Future<T> fut : INSTANCE.invokeAll(callables)) {
                result = fut.get();
            }
        } catch (ExecutionException | InterruptedException e) {
            System.err.println("FATAL: Future was interrupted");
            System.exit(1);
        } finally {
            return result;
        }
    }
}
