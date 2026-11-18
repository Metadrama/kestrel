package io.github.metadrama.kestrel;

import android.app.Activity;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.widget.EditText;
import android.widget.ListView;
import android.widget.TextView;
import android.widget.Toast;
import java.io.File;
import java.util.List;

public class MainActivity extends Activity implements DownloadAdapter.OnCardActionListener {
    private EditText etUrlInput;
    private TextView btnAdd;
    private ListView lvDownloads;
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
                    handler.post(() -> {
                        adapter.updateItems(snaps);
                        isUpdating = false;
                    });
                }).start();
            }
            handler.postDelayed(this, 100); // 10 Hz UI refresh ticker
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

        etUrlInput = findViewById(R.id.etUrlInput);
        btnAdd = findViewById(R.id.btnAdd);
        lvDownloads = findViewById(R.id.lvDownloads);

        adapter = new DownloadAdapter(this);
        lvDownloads.setAdapter(adapter);

        btnAdd.setOnClickListener(v -> {
            String url = etUrlInput.getText().toString().trim();
            if (!url.isEmpty()) {
                enqueueUrl(url);
                etUrlInput.setText("");
            }
        });

        // Handle incoming Share Sheet URL (text/plain)
        handleIntent(getIntent());
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
