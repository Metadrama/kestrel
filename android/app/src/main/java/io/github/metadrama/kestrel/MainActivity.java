package io.github.metadrama.kestrel;

import android.app.Activity;
import android.app.Dialog;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.drawable.ColorDrawable;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.view.Window;
import android.widget.EditText;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.ArrayList;
import java.util.List;

public class MainActivity extends Activity implements DownloadAdapter.OnCardActionListener {
    private ListView lvDownloads;
    private View btnFloatingAdd;
    private DownloadAdapter adapter;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean isUpdating = false;

    private final Runnable updateRunnable = new Runnable() {
        @Override
        public void run() {
            if (!isUpdating) {
                isUpdating = true;
                new Thread(() -> {
                    List<DownloadCardSnapshot> snaps = KestrelEngine.getSnapshots();
                    if (snaps == null || snaps.isEmpty()) {
                        snaps = createDemoSnapshots();
                    }
                    final List<DownloadCardSnapshot> finalSnaps = snaps;
                    handler.post(() -> {
                        adapter.updateItems(finalSnaps);
                        isUpdating = false;
                    });
                }).start();
            }
            handler.postDelayed(this, 200); // UI refresh ticker
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);

        // Initialize Native Kestrel Engine
        File dataDir = getFilesDir();
        File downloadDir = getExternalFilesDir(null);
        if (downloadDir == null) {
            downloadDir = new File(dataDir, "downloads");
        }
        downloadDir.mkdirs();

        KestrelEngine.nativeInit(dataDir.getAbsolutePath(), downloadDir.getAbsolutePath());

        // Start Foreground Download Service
        Intent serviceIntent = new Intent(this, DownloadService.class);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(serviceIntent);
        } else {
            startService(serviceIntent);
        }

        lvDownloads = findViewById(R.id.lvDownloads);
        btnFloatingAdd = findViewById(R.id.btnFloatingAdd);

        adapter = new DownloadAdapter(this);
        lvDownloads.setAdapter(adapter);

        btnFloatingAdd.setOnClickListener(v -> showAddUrlDialog());

        // Handle incoming Share Sheet URL (text/plain)
        handleIntent(getIntent());
    }

    private void showAddUrlDialog() {
        Dialog dialog = new Dialog(this);
        dialog.requestWindowFeature(Window.FEATURE_NO_TITLE);
        dialog.setContentView(R.layout.dialog_add_download);
        if (dialog.getWindow() != null) {
            dialog.getWindow().setBackgroundDrawable(new ColorDrawable(Color.TRANSPARENT));
        }

        EditText etDialogUrl = dialog.findViewById(R.id.etDialogUrl);
        TextView btnDialogCancel = dialog.findViewById(R.id.btnDialogCancel);
        TextView btnDialogDownload = dialog.findViewById(R.id.btnDialogDownload);

        // Auto paste from clipboard if URL is present
        try {
            ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (clipboard != null && clipboard.hasPrimaryClip()) {
                ClipData clipData = clipboard.getPrimaryClip();
                if (clipData != null && clipData.getItemCount() > 0) {
                    CharSequence text = clipData.getItemAt(0).getText();
                    if (text != null) {
                        String s = text.toString().trim();
                        if (s.startsWith("http://") || s.startsWith("https://")) {
                            etDialogUrl.setText(s);
                            etDialogUrl.setSelection(s.length());
                        }
                    }
                }
            }
        } catch (Exception ignored) {}

        btnDialogCancel.setOnClickListener(v -> dialog.dismiss());
        btnDialogDownload.setOnClickListener(v -> {
            String url = etDialogUrl.getText().toString().trim();
            if (!url.isEmpty()) {
                enqueueUrl(url);
                dialog.dismiss();
            } else {
                Toast.makeText(MainActivity.this, "Please enter a valid URL", Toast.LENGTH_SHORT).show();
            }
        });

        dialog.show();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleIntent(intent);
    }

    private void handleIntent(Intent intent) {
        if (intent == null) return;
        String action = intent.getAction();
        String type = intent.getType();

        if (Intent.ACTION_SEND.equals(action) && "text/plain".equals(type)) {
            String sharedText = intent.getStringExtra(Intent.EXTRA_TEXT);
            if (sharedText != null && !sharedText.trim().isEmpty()) {
                String candidate = sharedText.trim();
                int httpIdx = candidate.indexOf("http://");
                int httpsIdx = candidate.indexOf("https://");
                int start = -1;
                if (httpsIdx != -1) start = httpsIdx;
                else if (httpIdx != -1) start = httpIdx;

                if (start != -1) {
                    String sub = candidate.substring(start);
                    int spaceIdx = sub.indexOf(' ');
                    String url = spaceIdx != -1 ? sub.substring(0, spaceIdx) : sub;
                    enqueueUrl(url);
                    Toast.makeText(this, "Capturing stream in Kestrel...", Toast.LENGTH_SHORT).show();
                } else {
                    enqueueUrl(candidate);
                }
            }
        }
    }

    private void enqueueUrl(String url) {
        new Thread(() -> {
            String id = KestrelEngine.nativeEnqueue(url, "[]", "", true);
            handler.post(() -> {
                if (id != null) {
                    Toast.makeText(MainActivity.this, "Download enqueued", Toast.LENGTH_SHORT).show();
                } else {
                    Toast.makeText(MainActivity.this, "Failed to enqueue", Toast.LENGTH_SHORT).show();
                }
            });
        }).start();
    }

    private List<DownloadCardSnapshot> createDemoSnapshots() {
        List<DownloadCardSnapshot> list = new ArrayList<>();

        // 1. Active download matching mobile design mockup
        DownloadCardSnapshot active = new DownloadCardSnapshot();
        active.id = "demo-active-1";
        active.title = "ubuntu-26.04-desktop-amd64.iso";
        active.destPath = "~/Downloads";
        active.mimeCategory = "Binary";
        active.status = "Active";
        active.totalBytes = (long) (4.8 * 1024 * 1024 * 1024);
        active.downloadedBytes = (long) (active.totalBytes * 0.48);
        active.speedBps = (long) (142.8 * 1024 * 1024);
        active.etaSeconds = 18;
        active.sha256Verified = true;
        active.isExpanded = true;

        float[] ratios = {1.0f, 0.38f, 0.50f, 0.70f, 0.85f, 0.15f, 0.10f, 0.02f};
        for (int i = 0; i < 8; i++) {
            SegmentSlice slice = new SegmentSlice();
            slice.index = i;
            slice.startByte = 0;
            slice.endByte = 1000;
            slice.filledBytes = (long) (1000 * ratios[i]);
            slice.activeWorker = true;
            active.segments.add(slice);
        }
        list.add(active);

        // 2. Completed download matching mobile design mockup
        DownloadCardSnapshot completed = new DownloadCardSnapshot();
        completed.id = "demo-completed-1";
        completed.title = "Ableton_Live_12_Suite.dmg";
        completed.status = "Completed";
        completed.destPath = "~/Downloads";
        completed.sha256Verified = true;
        list.add(completed);

        return list;
    }

    @Override
    protected void onResume() {
        super.onResume();
        handler.post(updateRunnable);
    }

    @Override
    protected void onPause() {
        super.onPause();
        handler.removeCallbacks(updateRunnable);
    }

    @Override
    public void onTogglePause(DownloadCardSnapshot item) {
        new Thread(() -> {
            if ("Active".equalsIgnoreCase(item.status) || "Resolving".equalsIgnoreCase(item.status)) {
                KestrelEngine.nativePause(item.id);
            } else {
                KestrelEngine.nativeResume(item.id);
            }
        }).start();
    }

    @Override
    public void onCancel(DownloadCardSnapshot item) {
        new Thread(() -> {
            KestrelEngine.nativeCancel(item.id, false);
        }).start();
    }
}
