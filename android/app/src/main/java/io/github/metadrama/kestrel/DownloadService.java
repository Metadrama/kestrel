package io.github.metadrama.kestrel;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;
import java.util.List;

public class DownloadService extends Service {
    private static final String CHANNEL_ID = "kestrel_downloads";
    private static final int NOTIFICATION_ID = 1001;
    private PowerManager.WakeLock wakeLock;
    private boolean isRunning = false;

    @Override
    public void onCreate() {
        super.onCreate();
        createNotificationChannel();

        PowerManager powerManager = (PowerManager) getSystemService(Context.POWER_SERVICE);
        if (powerManager != null) {
            wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "Kestrel::DownloadWakeLock");
            wakeLock.acquire(12 * 60 * 60 * 1000L); // Max 12 hours
        }

        Notification notification = buildNotification("Kestrel Engine Active", "Ready for high-speed downloads");
        startForeground(NOTIFICATION_ID, notification);
        isRunning = true;
        startMonitorLoop();
    }

    private void startMonitorLoop() {
        new Thread(() -> {
            while (isRunning) {
                try {
                    Thread.sleep(1000);
                    List<DownloadCardSnapshot> snaps = KestrelEngine.getSnapshots();
                    boolean hasActive = false;
                    long totalSpeed = 0;
                    String activeTitle = "Downloading...";

                    for (DownloadCardSnapshot snap : snaps) {
                        if ("Active".equalsIgnoreCase(snap.status) || "Resolving".equalsIgnoreCase(snap.status)) {
                            hasActive = true;
                            totalSpeed += snap.speedBps;
                            activeTitle = snap.title;
                        }
                    }

                    if (hasActive) {
                        double mbps = (double) totalSpeed / (1024.0 * 1024.0);
                        String subtitle = String.format("%.1f MB/s · Multi-segment engine active", mbps);
                        Notification updated = buildNotification(activeTitle, subtitle);
                        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
                        if (nm != null) {
                            nm.notify(NOTIFICATION_ID, updated);
                        }
                    }
                } catch (InterruptedException e) {
                    break;
                }
            }
        }).start();
    }

    private Notification buildNotification(String title, String content) {
        Intent intent = new Intent(this, MainActivity.class);
        PendingIntent pi = PendingIntent.getActivity(this, 0, intent,
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.M ? PendingIntent.FLAG_IMMUTABLE : 0);

        Notification.Builder builder;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            builder = new Notification.Builder(this, CHANNEL_ID);
        } else {
            builder = new Notification.Builder(this);
        }

        return builder
                .setContentTitle(title)
                .setContentText(content)
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setContentIntent(pi)
                .setOngoing(true)
                .build();
    }

    private void createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL_ID,
                    "Kestrel Downloads",
                    NotificationManager.IMPORTANCE_LOW
            );
            channel.setDescription("Shows active download progress and speeds");
            NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            if (nm != null) {
                nm.createNotificationChannel(channel);
            }
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        isRunning = false;
        if (wakeLock != null && wakeLock.isHeld()) {
            wakeLock.release();
        }
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
