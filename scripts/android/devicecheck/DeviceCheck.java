package com.droidpc.devicecheck;

import android.app.Activity;
import android.os.Bundle;
import android.os.Handler;
import android.os.HandlerThread;
import android.hardware.camera2.*;
import android.graphics.ImageFormat;
import android.media.ImageReader;
import android.net.ConnectivityManager;
import android.net.Network;
import android.util.Log;
import android.util.Size;
import android.widget.TextView;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.Arrays;
import java.util.Collections;
import java.util.concurrent.atomic.AtomicBoolean;

public final class DeviceCheck extends Activity {
    private static final String TAG = "DroidpcDeviceCheck";
    private final AtomicBoolean cameraDone = new AtomicBoolean();
    private HandlerThread worker;
    private CameraDevice camera;
    private ImageReader reader;
    private CameraCaptureSession session;
    private int completed;
    private TextView status;
    private String runId;

    private void report(String message) {
        Log.i(TAG, runId + " " + message);
        runOnUiThread(() -> status.append("\n" + message));
    }
    private void done() {
        runOnUiThread(() -> { if (++completed == 2) { report("CHECK_COMPLETE"); finish(); } });
    }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        runId = getIntent().getStringExtra("run");
        status = new TextView(this);
        status.setPadding(24, 24, 24, 24);
        status.setText("Checking camera and internet…");
        setContentView(status);
        worker = new HandlerThread("device-check");
        worker.start();
        Handler handler = new Handler(worker.getLooper());
        new Thread(this::checkNetwork, "network-check").start();
        handler.postDelayed(this::checkCamera, 500);
        handler.postDelayed(() -> cameraResult("CAMERA_FAIL timeout"), 20000);
    }
    private void checkNetwork() {
        HttpURLConnection connection = null;
        try {
            ConnectivityManager manager = getSystemService(ConnectivityManager.class);
            Network network = manager.getActiveNetwork();
            report("DEFAULT_NETWORK=" + network);
            if (network == null) throw new IllegalStateException("Android has no default network");
            report("NETWORK_CAPABILITIES=" + manager.getNetworkCapabilities(network));
            report("NETWORK_LINK=" + manager.getLinkProperties(network));
            connection = (HttpURLConnection) network.openConnection(new URL("https://connectivitycheck.gstatic.com/generate_204"));
            connection.setConnectTimeout(6000);
            connection.setReadTimeout(6000);
            connection.setInstanceFollowRedirects(false);
            int code = connection.getResponseCode();
            report((code == 204 ? "HTTPS_OK " : "HTTPS_FAIL status=") + code);
        } catch (Exception error) { report("HTTPS_FAIL " + error); }
        finally { if (connection != null) connection.disconnect(); done(); }
    }
    private void cameraResult(String result) {
        if (!cameraDone.compareAndSet(false, true)) return;
        report(result);
        if (session != null) session.close();
        if (camera != null) camera.close();
        if (reader != null) reader.close();
        done();
    }
    private void checkCamera() {
        try {
            CameraManager manager = getSystemService(CameraManager.class);
            String[] ids = manager.getCameraIdList();
            report("CAMERA_IDS=" + Arrays.toString(ids));
            if (ids.length == 0) throw new IllegalStateException("No camera registered by Android HAL");
            CameraCharacteristics characteristics = manager.getCameraCharacteristics(ids[0]);
            report("CAMERA_FACING=" + characteristics.get(CameraCharacteristics.LENS_FACING));
            report("CAMERA_HARDWARE_LEVEL=" + characteristics.get(CameraCharacteristics.INFO_SUPPORTED_HARDWARE_LEVEL));
            Size[] sizes = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP).getOutputSizes(ImageFormat.JPEG);
            if (sizes == null || sizes.length == 0) throw new IllegalStateException("No JPEG output sizes");
            Size size = Collections.min(Arrays.asList(sizes), (a, b) -> Long.compare((long)a.getWidth()*a.getHeight(), (long)b.getWidth()*b.getHeight()));
            Handler handler = new Handler(worker.getLooper());
            reader = ImageReader.newInstance(size.getWidth(), size.getHeight(), ImageFormat.JPEG, 2);
            reader.setOnImageAvailableListener(r -> {
                try (android.media.Image image = r.acquireLatestImage()) {
                    if (image == null) return;
                    java.nio.ByteBuffer jpeg = image.getPlanes()[0].getBuffer();
                    int bytes = jpeg.remaining();
                    boolean valid = bytes > 4 && (jpeg.get(jpeg.position()) & 255) == 255 && (jpeg.get(jpeg.position() + 1) & 255) == 216;
                    cameraResult((valid ? "CAMERA_FRAME_OK " : "CAMERA_FAIL empty frame ") + size + " bytes=" + bytes);
                } catch (Exception error) { cameraResult("CAMERA_FAIL " + error); }
            }, handler);
            manager.openCamera(ids[0], new CameraDevice.StateCallback() {
                @Override public void onOpened(CameraDevice device) {
                    if (cameraDone.get()) { device.close(); return; }
                    camera = device;
                    try {
                        device.createCaptureSession(Collections.singletonList(reader.getSurface()), new CameraCaptureSession.StateCallback() {
                            @Override public void onConfigured(CameraCaptureSession configured) {
                                if (cameraDone.get()) { configured.close(); return; }
                                session = configured;
                                try {
                                    CaptureRequest.Builder request = device.createCaptureRequest(CameraDevice.TEMPLATE_STILL_CAPTURE);
                                    request.addTarget(reader.getSurface());
                                    configured.capture(request.build(), null, handler);
                                } catch (Exception error) { cameraResult("CAMERA_FAIL " + error); }
                            }
                            @Override public void onConfigureFailed(CameraCaptureSession failed) { failed.close(); cameraResult("CAMERA_FAIL session configuration"); }
                        }, handler);
                    } catch (Exception error) { cameraResult("CAMERA_FAIL " + error); }
                }
                @Override public void onDisconnected(CameraDevice device) { device.close(); cameraResult("CAMERA_FAIL disconnected"); }
                @Override public void onError(CameraDevice device, int error) { device.close(); cameraResult("CAMERA_FAIL open error=" + error); }
            }, handler);
        } catch (Exception error) { cameraResult("CAMERA_FAIL " + error); }
    }
    @Override public void onDestroy() {
        if (session != null) session.close();
        if (camera != null) camera.close();
        if (reader != null) reader.close();
        if (worker != null) worker.quitSafely();
        super.onDestroy();
    }
}
