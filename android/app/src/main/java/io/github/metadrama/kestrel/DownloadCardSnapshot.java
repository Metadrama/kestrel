package io.github.metadrama.kestrel;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

public class DownloadCardSnapshot {
    public String id;
    public String title;
    public String destPath;
    public String mimeCategory;
    public String status;
    public long totalBytes;
    public long downloadedBytes;
    public long speedBps;
    public long etaSeconds;
    public boolean sha256Verified;
    public String errorSummary;
    public List<SegmentSlice> segments = new ArrayList<>();
    public boolean isExpanded = true;

    public boolean isCompleted() {
        return "Done".equalsIgnoreCase(status) || "Completed".equalsIgnoreCase(status);
    }

    public float getProgress() {
        if (totalBytes > 0) {
            return Math.max(0f, Math.min(1f, (float) downloadedBytes / (float) totalBytes));
        }
        return 0f;
    }

    public String getFormattedSpeed() {
        double bps = (double) speedBps;
        if (bps >= 1024.0 * 1024.0 * 1024.0) {
            return String.format(Locale.US, "%.1f GB/s", bps / (1024.0 * 1024.0 * 1024.0));
        } else if (bps >= 1024.0 * 1024.0) {
            return String.format(Locale.US, "%.1f MB/s", bps / (1024.0 * 1024.0));
        } else if (bps >= 1024.0) {
            return String.format(Locale.US, "%.1f KB/s", bps / 1024.0);
        } else {
            return speedBps + " B/s";
        }
    }

    public String getFormattedSize() {
        long bytes = totalBytes > 0 ? totalBytes : downloadedBytes;
        double d = (double) bytes;
        if (d >= 1024.0 * 1024.0 * 1024.0) {
            return String.format(Locale.US, "%.1f GB", d / (1024.0 * 1024.0 * 1024.0));
        } else if (d >= 1024.0 * 1024.0) {
            return String.format(Locale.US, "%.1f MB", d / (1024.0 * 1024.0));
        } else if (d >= 1024.0) {
            return String.format(Locale.US, "%.1f KB", d / 1024.0);
        } else {
            return bytes + " B";
        }
    }

    public String getFormattedEta() {
        if (etaSeconds > 0) {
            if (etaSeconds >= 3600) {
                return String.format(Locale.US, "%dh %dm left", etaSeconds / 3600, (etaSeconds % 3600) / 60);
            } else if (etaSeconds >= 60) {
                return String.format(Locale.US, "%dm %ds left", etaSeconds / 60, etaSeconds % 60);
            } else {
                return String.format(Locale.US, "%ds left", etaSeconds);
            }
        }
        return "--";
    }

    public String getTelemetry() {
        return getFormattedSpeed() + " · " + getFormattedEta() + " · " + getFormattedSize();
    }

    public String getDisplayPath() {
        if (destPath == null || destPath.trim().isEmpty() || destPath.contains("downloads")) {
            return "~/Downloads";
        }
        return destPath;
    }
}
