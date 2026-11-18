package io.github.metadrama.kestrel;

import org.json.JSONArray;
import org.json.JSONObject;
import java.util.ArrayList;
import java.util.List;

public class KestrelEngine {
    static {
        try {
            System.loadLibrary("kestrel_mobile");
        } catch (UnsatisfiedLinkError e) {
            e.printStackTrace();
        }
    }

    public static native boolean nativeInit(String dataDir, String downloadDir);
    public static native String nativeEnqueue(String url, String headersJson, String destDir, boolean autoStart);
    public static native boolean nativePause(String id);
    public static native boolean nativeResume(String id);
    public static native boolean nativeCancel(String id, boolean deleteFile);
    public static native String nativeGetSnapshotsJson();

    public static List<DownloadCardSnapshot> getSnapshots() {
        List<DownloadCardSnapshot> list = new ArrayList<>();
        String jsonStr = nativeGetSnapshotsJson();
        if (jsonStr == null || jsonStr.isEmpty()) {
            return list;
        }

        try {
            JSONArray arr = new JSONArray(jsonStr);
            for (int i = 0; i < arr.length(); i++) {
                JSONObject obj = arr.getJSONObject(i);
                DownloadCardSnapshot snap = new DownloadCardSnapshot();
                snap.id = obj.getString("id");
                snap.title = obj.optString("title", "download");
                snap.destPath = obj.optString("dest_path", "");
                snap.mimeCategory = obj.optString("mime_category", "Other");
                snap.status = obj.optString("status", "Queued");
                snap.totalBytes = obj.has("total_bytes") && !obj.isNull("total_bytes") ? obj.getLong("total_bytes") : -1;
                snap.downloadedBytes = obj.optLong("downloaded_bytes", 0);
                snap.speedBps = obj.optLong("speed_bps", 0);
                snap.etaSeconds = obj.has("eta_seconds") && !obj.isNull("eta_seconds") ? obj.getLong("eta_seconds") : -1;
                snap.sha256Verified = obj.has("sha256_verified") && !obj.isNull("sha256_verified") ? obj.getBoolean("sha256_verified") : false;
                snap.errorSummary = obj.optString("error_summary", null);

                JSONArray segs = obj.optJSONArray("segments");
                if (segs != null) {
                    for (int j = 0; j < segs.length(); j++) {
                        JSONObject sObj = segs.getJSONObject(j);
                        SegmentSlice slice = new SegmentSlice();
                        slice.index = sObj.getInt("index");
                        slice.startByte = sObj.getLong("start_byte");
                        slice.endByte = sObj.getLong("end_byte");
                        slice.filledBytes = sObj.getLong("filled_bytes");
                        slice.activeWorker = sObj.getBoolean("active_worker");
                        snap.segments.add(slice);
                    }
                }
                list.add(snap);
            }
        } catch (Exception e) {
            e.printStackTrace();
        }
        return list;
    }
}
