package io.github.metadrama.kestrel;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;
import java.util.ArrayList;
import java.util.List;

public class EqualizerSegmentsView extends View {
    private static final int THREAD_COUNT = 8;
    private final float[] threadRatios = new float[THREAD_COUNT];
    
    private final Paint trackPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint borderPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint fillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    
    private final RectF cellRect = new RectF();
    private final RectF fillRect = new RectF();
    private final Path cellPath = new Path();

    public EqualizerSegmentsView(Context context) {
        super(context);
        init();
    }

    public EqualizerSegmentsView(Context context, AttributeSet attrs) {
        super(context, attrs);
        init();
    }

    public EqualizerSegmentsView(Context context, AttributeSet attrs, int defStyleAttr) {
        super(context, attrs, defStyleAttr);
        init();
    }

    private void init() {
        trackPaint.setStyle(Paint.Style.FILL);
        trackPaint.setColor(Color.parseColor("#161920"));

        borderPaint.setStyle(Paint.Style.STROKE);
        borderPaint.setStrokeWidth(dpToPx(1f));
        borderPaint.setColor(Color.parseColor("#252A36"));

        fillPaint.setStyle(Paint.Style.FILL);
        fillPaint.setColor(Color.parseColor("#7CB4D8")); // Ice-blue
        
        // Default demo ratios matching the mobile design screenshot
        float[] demo = {1.0f, 0.38f, 0.50f, 0.70f, 0.85f, 0.15f, 0.10f, 0.02f};
        System.arraycopy(demo, 0, threadRatios, 0, THREAD_COUNT);
    }

    public void setSegments(List<SegmentSlice> slices) {
        if (slices != null && !slices.isEmpty()) {
            int count = slices.size();
            for (int i = 0; i < THREAD_COUNT; i++) {
                if (i < count) {
                    threadRatios[i] = slices.get(i).getFillRatio();
                } else {
                    threadRatios[i] = 0.0f;
                }
            }
        }
        invalidate();
    }

    public void setThreadRatios(float[] ratios) {
        if (ratios != null) {
            for (int i = 0; i < THREAD_COUNT; i++) {
                threadRatios[i] = i < ratios.length ? Math.max(0f, Math.min(1f, ratios[i])) : 0f;
            }
            invalidate();
        }
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        float w = getWidth();
        float h = getHeight();
        if (w <= 0 || h <= 0) return;

        float spacing = dpToPx(6f);
        float cornerRadius = dpToPx(5f);
        float totalSpacing = spacing * (THREAD_COUNT - 1);
        float colWidth = (w - totalSpacing) / THREAD_COUNT;

        for (int i = 0; i < THREAD_COUNT; i++) {
            float startX = i * (colWidth + spacing);
            float endX = startX + colWidth;

            cellRect.set(startX, 0, endX, h);

            cellPath.reset();
            cellPath.addRoundRect(cellRect, cornerRadius, cornerRadius, Path.Direction.CW);

            canvas.save();
            canvas.clipPath(cellPath);

            // Draw dark background track
            canvas.drawRect(cellRect, trackPaint);

            // Draw upward-filling ice-blue progress
            float ratio = threadRatios[i];
            if (ratio > 0.005f) {
                float fillTop = h - (h * ratio);
                fillRect.set(startX, fillTop, endX, h);
                canvas.drawRect(fillRect, fillPaint);
            }

            canvas.restore();

            // Draw crisp subtle border around the column
            canvas.drawRoundRect(cellRect, cornerRadius, cornerRadius, borderPaint);
        }
    }

    private float dpToPx(float dp) {
        return dp * getResources().getDisplayMetrics().density;
    }
}
