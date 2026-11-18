package io.github.metadrama.kestrel;

public class SegmentSlice {
    public int index;
    public long startByte;
    public long endByte;
    public long filledBytes;
    public boolean activeWorker;

    public float getFillRatio() {
        long total = endByte - startByte;
        if (total <= 0) return 1.0f;
        float r = (float) filledBytes / (float) total;
        return Math.max(0.0f, Math.min(1.0f, r));
    }
}
