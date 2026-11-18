package io.github.metadrama.kestrel;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;

public class ThinProgressBar extends View {
    private float progress = 0.5f;
    private final Paint trackPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint fillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF trackRect = new RectF();
    private final RectF fillRect = new RectF();

    public ThinProgressBar(Context context) {
        super(context);
        init();
    }

    public ThinProgressBar(Context context, AttributeSet attrs) {
        super(context, attrs);
        init();
    }

    public ThinProgressBar(Context context, AttributeSet attrs, int defStyleAttr) {
        super(context, attrs, defStyleAttr);
        init();
    }

    private void init() {
        trackPaint.setStyle(Paint.Style.FILL);
        trackPaint.setColor(Color.parseColor("#232732"));

        fillPaint.setStyle(Paint.Style.FILL);
        fillPaint.setColor(Color.parseColor("#7CB4D8")); // Ice-blue
    }

    public void setProgress(float progress) {
        this.progress = Math.max(0f, Math.min(1f, progress));
        invalidate();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        float w = getWidth();
        float h = getHeight();
        if (w <= 0 || h <= 0) return;

        float corner = h / 2f;
        trackRect.set(0, 0, w, h);
        canvas.drawRoundRect(trackRect, corner, corner, trackPaint);

        if (progress > 0.005f) {
            fillRect.set(0, 0, w * progress, h);
            canvas.drawRoundRect(fillRect, corner, corner, fillPaint);
        }
    }
}
