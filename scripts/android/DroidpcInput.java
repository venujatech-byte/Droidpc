import android.os.SystemClock;
import android.view.InputDevice;
import android.view.InputEvent;
import android.view.MotionEvent;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.lang.reflect.Method;
import java.util.Arrays;

/** Persistent wheel injection: no touch slop, synthetic flings or process per tick. */
public final class DroidpcInput {
    private static Object manager;
    private static Method inject;

    static float number(String value, float min, float max) {
        float parsed = Float.parseFloat(value);
        if (!Float.isFinite(parsed) || parsed < min || parsed > max)
            throw new IllegalArgumentException("input value outside bounds");
        return parsed;
    }

    static String[] command(String line) {
        if (line.length() > 1024) throw new IllegalArgumentException("input line too long");
        String[] args = line.trim().split("\\s+");
        switch (args[0]) {
            case "scroll":
                if (args.length != 5) throw new IllegalArgumentException();
                number(args[1], 0, 8191); number(args[2], 0, 8191);
                number(args[3], -4, 4); number(args[4], -4, 4);
                break;
            case "tap":
                if (args.length != 3) throw new IllegalArgumentException();
                number(args[1], 0, 8191); number(args[2], 0, 8191);
                break;
            case "swipe":
                if (args.length != 6) throw new IllegalArgumentException();
                for (int i = 1; i < 5; i++) number(args[i], 0, 8191);
                number(args[5], 1, 1000);
                break;
            case "key":
                if (args.length != 2 || !args[1].matches("[0-9]{1,3}"))
                    throw new IllegalArgumentException();
                break;
            case "text":
                if (args.length != 2 || !args[1].matches("[\\x21-\\x7e]+"))
                    throw new IllegalArgumentException();
                break;
            default: throw new IllegalArgumentException("unknown input command");
        }
        return args;
    }

    private static void scroll(String[] args) throws Exception {
        if (manager == null) {
            Class<?> type = Class.forName("android.hardware.input.InputManager");
            manager = type.getMethod("getInstance").invoke(null);
            inject = type.getMethod("injectInputEvent", InputEvent.class, int.class);
        }
        MotionEvent.PointerProperties properties = new MotionEvent.PointerProperties();
        properties.id = 0;
        properties.toolType = MotionEvent.TOOL_TYPE_MOUSE;
        MotionEvent.PointerCoords coords = new MotionEvent.PointerCoords();
        coords.x = Float.parseFloat(args[1]);
        coords.y = Float.parseFloat(args[2]);
        coords.setAxisValue(MotionEvent.AXIS_HSCROLL, Float.parseFloat(args[3]));
        coords.setAxisValue(MotionEvent.AXIS_VSCROLL, Float.parseFloat(args[4]));
        long now = SystemClock.uptimeMillis();
        MotionEvent event = MotionEvent.obtain(now, now, MotionEvent.ACTION_SCROLL,
                1, new MotionEvent.PointerProperties[]{properties},
                new MotionEvent.PointerCoords[]{coords}, 0, 0, 1, 1, -1, 0,
                InputDevice.SOURCE_MOUSE, 0);
        try {
            MotionEvent hover = MotionEvent.obtain(event);
            hover.setAction(MotionEvent.ACTION_HOVER_MOVE);
            try { inject.invoke(manager, hover, 0); }
            finally { hover.recycle(); }
            if (!Boolean.TRUE.equals(inject.invoke(manager, event, 0)))
                throw new IllegalStateException("Android rejected scroll event");
        } finally { event.recycle(); }
    }

    public static void main(String[] ignored) throws Exception {
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        for (String line; (line = reader.readLine()) != null;) {
            try {
                String[] args = command(line);
                if (args[0].equals("scroll")) {
                    scroll(args);
                } else {
                    String[] argv = new String[args.length + 1];
                    argv[0] = "/system/bin/input";
                    System.arraycopy(args, 0, argv, 1, args.length);
                    if (argv[1].equals("key")) argv[1] = "keyevent";
                    Process process = new ProcessBuilder(Arrays.asList(argv)).inheritIO().start();
                    if (process.waitFor() != 0) System.err.println("Android input command failed");
                }
            } catch (Exception error) {
                System.err.println("Android input: " + error);
            }
        }
    }
}
