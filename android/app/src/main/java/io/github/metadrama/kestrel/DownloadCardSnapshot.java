package io.github.metadrama.kestrel;

import java.util.ArrayList;
import java.util.List;

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
    public boolean isExpanded = false;

    public String getFormattedSpeed() {
        double bps = (double) speedBps;
        if (bps >= 1024.0 * 1024.0 * 1024.0) {
            return String.format("%.1f GB/s", bps / (1024.0 * 1024.0 * 1024.0));
        } else if (bps >= 1024.0 * 1024.0) {
            return String.format("%.1f MB/s", bps / (1024.0 * 1024.0));
        } else if (bps >= 1024.0) {
            return String.format("%.1f KB/s", bps / 1024.0);
        } else {
            return speedBps + " B/s";
        }
    }

    public String getFormattedSize() {
        double cur = (double) downloadedBytes;
        if (totalBytes > 0) {
            double tot = (double) totalBytes;
            if (tot >= 1024.0 * 1024.0 * 1024.0) {
                return String.format("%.1f / %.1f GB", cur / 1024.0 / 1024.0 / 1024.0, tot / 1024.0 / 1024.0 / 1024.0);
            } else if (tot >= 1024.0 * 1024.0) {
                return String.format("%.1f / %.1f MB", cur / 1024.0 / 1024.0, tot / 1024.0 / 1024.0);
            } else {
                return String.format("%.0f / %.0f KB", cur / 1024.0, tot / 1024.0);
            }
        } else {
            if (cur >= 1024.0 * 1024.0) {
                return String.format("%.1f MB", cur / 1024.0 / 1024.0);
            } else {
                return String.format("%.0f KB", cur / 1024.0);
            }
        }
    }

    public String getFormattedEta() {
        if (etaSeconds > 0) {
            if (etaSeconds >= 3600) {
                return String.format("%dh %dm left", etaSeconds / 3600, (etaSeconds % 3600) / 60);
            } else if (etaSeconds >= 60) {
                return String.format("%dm %ds left", etaSeconds / 60, etaSeconds % 60);
            } else {
                return String.format("%ds left", etaSeconds);
            }
        }
        return "--";
    }

    public String getTelemetry() {
        return getFormattedSpeed() + " · " + getFormattedEta() + " · " + getFormattedSize();
    }

    public String getMimeBadge() {
        if ("Video".equalsIgnoreCase(mimeCategory)) return "VID";
        if ("Audio".equalsIgnoreCase(mimeCategory)) return "AUD";
        if ("Archive".equalsIgnoreCase(mimeCategory)) return "ZIP";
        if ("Document".equalsIgnoreCase(mimeCategory)) return "DOC";
        if ("Binary".equalsIgnoreCase(mimeCategory)) return "BIN";
        if ("Image".equalsIgnoreCase(mimeCategory)) return "IMG";
        return "FILE";
    }
}
