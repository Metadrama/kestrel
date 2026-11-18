package io.github.metadrama.kestrel;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;
import java.util.ArrayList;
import java.util.List;

public class MultiSegmentProgressBar extends View {
    private final List<SegmentSlice> segments = new ArrayList<>();
    private final Paint bgPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint cellBgPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint activeFillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint idleFillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF trackRect = new RectF();
    private final RectF cellRect = new RectF();
    private final RectF fillRect = new RectF();

    public MultiSegmentProgressBar(Context context) {
        super(context);
        init();
    }

    public MultiSegmentProgressBar(Context context, AttributeSet attrs) {
        super(context, attrs);
        init();
    }

    public MultiSegmentProgressBar(Context context, AttributeSet attrs, int defStyleAttr) {
        super(context, attrs, defStyleAttr);
        init();
    }

    private void init() {
        bgPaint.setColor(Color.parseColor("#0E1013"));
        cellBgPaint.setColor(Color.parseColor("#1B1E24"));
        activeFillPaint.setColor(Color.parseColor("#7CB4D8")); // Ice-blue
        idleFillPaint.setColor(Color.parseColor("#4A708B"));
    }

    public void setSegments(List<SegmentSlice> slices) {
        this.segments.clear();
        if (slices != null) {
            this.segments.addAll(slices);
        }
        invalidate();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        float w = getWidth();
        float h = getHeight();
        float corner = h / 2.0f;

        trackRect.set(0, 0, w, h);
        canvas.drawRoundRect(trackRect, corner, corner, bgPaint);

        int count = segments.size();
        if (count == 0) {
            return;
        }

        float spacing = 3.0f;
        float totalSpacing = spacing * (count - 1);
        float cellWidth = (w - totalSpacing) / count;

        for (int i = 0; i < count; i++) {
            SegmentSlice slice = segments.get(i);
            float startX = i * (cellWidth + spacing);
            float endX = startX + cellWidth;

            cellRect.set(startX, 0, endX, h);
            canvas.drawRoundRect(cellRect, 4.0f, 4.0f, cellBgPaint);

            float ratio = slice.getFillRatio();
            if (ratio > 0.0f) {
                float fillEndX = startX + (cellWidth * ratio);
                fillRect.set(startX, 0, fillEndX, h);
                Paint p = slice.activeWorker ? activeFillPaint : idleFillPaint;
                canvas.drawRoundRect(fillRect, 4.0f, 4.0f, p);
            }
        }
    }
}
