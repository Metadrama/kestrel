package io.github.metadrama.kestrel;

import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.BaseAdapter;
import android.widget.TextView;
import java.util.ArrayList;
import java.util.List;

public class DownloadAdapter extends BaseAdapter {
    private static final int VIEW_TYPE_ACTIVE = 0;
    private static final int VIEW_TYPE_COMPLETED = 1;

    public interface OnCardActionListener {
        void onTogglePause(DownloadCardSnapshot item);
        void onCancel(DownloadCardSnapshot item);
    }

    private final List<DownloadCardSnapshot> items = new ArrayList<>();
    private final OnCardActionListener listener;

    public DownloadAdapter(OnCardActionListener listener) {
        this.listener = listener;
    }

    public void updateItems(List<DownloadCardSnapshot> newItems) {
        this.items.clear();
        if (newItems != null) {
            this.items.addAll(newItems);
        }
        notifyDataSetChanged();
    }

    @Override
    public int getCount() {
        return items.size();
    }

    @Override
    public DownloadCardSnapshot getItem(int position) {
        return items.get(position);
    }

    @Override
    public long getItemId(int position) {
        return position;
    }

    @Override
    public int getViewTypeCount() {
        return 2;
    }

    @Override
    public int getItemViewType(int position) {
        DownloadCardSnapshot item = getItem(position);
        return item.isCompleted() ? VIEW_TYPE_COMPLETED : VIEW_TYPE_ACTIVE;
    }

    @Override
    public View getView(int position, View convertView, ViewGroup parent) {
        int viewType = getItemViewType(position);
        DownloadCardSnapshot item = getItem(position);

        if (viewType == VIEW_TYPE_COMPLETED) {
            CompletedViewHolder holder;
            if (convertView == null) {
                convertView = LayoutInflater.from(parent.getContext())
                        .inflate(R.layout.item_completed_card, parent, false);
                holder = new CompletedViewHolder();
                holder.tvCompletedIcon = convertView.findViewById(R.id.tvCompletedIcon);
                holder.tvCompletedTitle = convertView.findViewById(R.id.tvCompletedTitle);
                holder.tvCompletedStatus = convertView.findViewById(R.id.tvCompletedStatus);
                convertView.setTag(holder);
            } else {
                holder = (CompletedViewHolder) convertView.getTag();
            }

            holder.tvCompletedTitle.setText(item.title);
            holder.tvCompletedStatus.setText(parent.getContext().getString(R.string.completed));
            return convertView;
        }

        ActiveViewHolder holder;
        if (convertView == null) {
            convertView = LayoutInflater.from(parent.getContext())
                    .inflate(R.layout.item_download_card, parent, false);
            holder = new ActiveViewHolder();
            holder.tvTitle = convertView.findViewById(R.id.tvTitle);
            holder.progressLine = convertView.findViewById(R.id.progressLine);
            holder.tvTelemetry = convertView.findViewById(R.id.tvTelemetry);
            holder.btnRingAction = convertView.findViewById(R.id.btnRingAction);
            
            holder.layoutSegmentsHeader = convertView.findViewById(R.id.layoutSegmentsHeader);
            holder.tvSegmentsTitle = convertView.findViewById(R.id.tvSegmentsTitle);
            holder.tvSegmentsChevron = convertView.findViewById(R.id.tvSegmentsChevron);
            holder.layoutSegmentsContent = convertView.findViewById(R.id.layoutSegmentsContent);
            holder.equalizerView = convertView.findViewById(R.id.equalizerView);
            holder.tvFolderDest = convertView.findViewById(R.id.tvFolderDest);
            holder.tvShaBadge = convertView.findViewById(R.id.tvShaBadge);
            convertView.setTag(holder);
        } else {
            holder = (ActiveViewHolder) convertView.getTag();
        }

        holder.tvTitle.setText(item.title);
        holder.tvTelemetry.setText(item.getTelemetry());

        float progress = item.getProgress();
        holder.progressLine.setProgress(progress);

        boolean isActive = "Active".equalsIgnoreCase(item.status) || "Resolving".equalsIgnoreCase(item.status);
        holder.btnRingAction.setActive(isActive);
        holder.btnRingAction.setProgress(progress);
        holder.btnRingAction.setOnClickListener(v -> {
            if (listener != null) {
                listener.onTogglePause(item);
            }
        });

        // Segments equalizer bars
        holder.equalizerView.setSegments(item.segments);

        // Chevron collapse/expand toggle
        holder.layoutSegmentsContent.setVisibility(item.isExpanded ? View.VISIBLE : View.GONE);
        holder.tvSegmentsChevron.setText(item.isExpanded ? "⌄" : "›");

        View.OnClickListener toggleChevron = v -> {
            item.isExpanded = !item.isExpanded;
            holder.layoutSegmentsContent.setVisibility(item.isExpanded ? View.VISIBLE : View.GONE);
            holder.tvSegmentsChevron.setText(item.isExpanded ? "⌄" : "›");
        };
        holder.layoutSegmentsHeader.setOnClickListener(toggleChevron);

        // Footer pills
        holder.tvFolderDest.setText(item.getDisplayPath());
        holder.tvShaBadge.setVisibility(item.sha256Verified ? View.VISIBLE : View.GONE);

        return convertView;
    }

    static class ActiveViewHolder {
        TextView tvTitle;
        ThinProgressBar progressLine;
        TextView tvTelemetry;
        ProgressRingButton btnRingAction;
        View layoutSegmentsHeader;
        TextView tvSegmentsTitle;
        TextView tvSegmentsChevron;
        View layoutSegmentsContent;
        EqualizerSegmentsView equalizerView;
        TextView tvFolderDest;
        TextView tvShaBadge;
    }

    static class CompletedViewHolder {
        TextView tvCompletedIcon;
        TextView tvCompletedTitle;
        TextView tvCompletedStatus;
    }
}
