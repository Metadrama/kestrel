package io.github.metadrama.kestrel;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;

public class ProgressRingButton extends View {
    private boolean isActive = true;
    private float progress = 0.5f;

    private final Paint trackPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint arcPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint innerPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint bgPaint = new Paint(Paint.ANTI_ALIAS_FLAG);

    private final RectF ringRect = new RectF();
    private final Path iconPath = new Path();

    public ProgressRingButton(Context context) {
        super(context);
        init();
    }

    public ProgressRingButton(Context context, AttributeSet attrs) {
        super(context, attrs);
        init();
    }

    public ProgressRingButton(Context context, AttributeSet attrs, int defStyleAttr) {
        super(context, attrs, defStyleAttr);
        init();
    }

    private void init() {
        setClickable(true);
        setFocusable(true);

        bgPaint.setStyle(Paint.Style.FILL);
        bgPaint.setColor(Color.parseColor("#14171E"));

        trackPaint.setStyle(Paint.Style.STROKE);
        trackPaint.setStrokeWidth(dpToPx(2.5f));
        trackPaint.setColor(Color.parseColor("#232732"));

        arcPaint.setStyle(Paint.Style.STROKE);
        arcPaint.setStrokeWidth(dpToPx(2.5f));
        arcPaint.setStrokeCap(Paint.Cap.ROUND);
        arcPaint.setColor(Color.parseColor("#7CB4D8")); // Ice-blue

        innerPaint.setStyle(Paint.Style.FILL);
        innerPaint.setColor(Color.parseColor("#7CB4D8")); // Ice-blue
    }

    public void setActive(boolean active) {
        this.isActive = active;
        invalidate();
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

        float stroke = dpToPx(2.5f);
        float pad = stroke / 2f + dpToPx(2f);
        ringRect.set(pad, pad, w - pad, h - pad);

        // Circular background
        canvas.drawCircle(w / 2f, h / 2f, (w / 2f) - pad, bgPaint);

        // Outer circular track
        canvas.drawOval(ringRect, trackPaint);

        // Progress arc
        if (progress > 0.001f) {
            float sweepAngle = progress * 360f;
            canvas.drawArc(ringRect, -90f, sweepAngle, false, arcPaint);
        }

        // Center Icon
        float cx = w / 2f;
        float cy = h / 2f;
        if (isActive) {
            // Pause icon: two vertical rounded pills
            float barW = dpToPx(2.5f);
            float barH = dpToPx(11f);
            float gap = dpToPx(3.5f);
            float corner = barW / 2f;

            // Left bar
            RectF leftBar = new RectF(cx - gap / 2f - barW, cy - barH / 2f, cx - gap / 2f, cy + barH / 2f);
            canvas.drawRoundRect(leftBar, corner, corner, innerPaint);

            // Right bar
            RectF rightBar = new RectF(cx + gap / 2f, cy - barH / 2f, cx + gap / 2f + barW, cy + barH / 2f);
            canvas.drawRoundRect(rightBar, corner, corner, innerPaint);
        } else {
            // Play icon: right-pointing triangle
            float playSize = dpToPx(9f);
            iconPath.reset();
            iconPath.moveTo(cx - playSize * 0.4f, cy - playSize * 0.6f);
            iconPath.lineTo(cx + playSize * 0.6f, cy);
            iconPath.lineTo(cx - playSize * 0.4f, cy + playSize * 0.6f);
            iconPath.close();
            canvas.drawPath(iconPath, innerPaint);
        }
    }

    private float dpToPx(float dp) {
        return dp * getResources().getDisplayMetrics().density;
    }
}
