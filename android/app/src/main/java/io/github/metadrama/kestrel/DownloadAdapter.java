package io.github.metadrama.kestrel;

import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.BaseAdapter;
import android.widget.TextView;
import java.util.ArrayList;
import java.util.List;

public class DownloadAdapter extends BaseAdapter {

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
    public View getView(int position, View convertView, ViewGroup parent) {
        ViewHolder holder;
        if (convertView == null) {
            convertView = LayoutInflater.from(parent.getContext())
                    .inflate(R.layout.item_download_card, parent, false);
            holder = new ViewHolder();
            holder.tvMimeBadge = convertView.findViewById(R.id.tvMimeBadge);
            holder.tvTitle = convertView.findViewById(R.id.tvTitle);
            holder.tvTelemetry = convertView.findViewById(R.id.tvTelemetry);
            holder.btnAction = convertView.findViewById(R.id.btnAction);
            holder.segmentBar = convertView.findViewById(R.id.segmentBar);
            holder.layoutExpanded = convertView.findViewById(R.id.layoutExpanded);
            holder.tvDestPath = convertView.findViewById(R.id.tvDestPath);
            holder.tvShaBadge = convertView.findViewById(R.id.tvShaBadge);
            convertView.setTag(holder);
        } else {
            holder = (ViewHolder) convertView.getTag();
        }

        DownloadCardSnapshot item = getItem(position);
        holder.tvMimeBadge.setText(item.getMimeBadge());
        holder.tvTitle.setText(item.title);
        holder.tvTelemetry.setText(item.getTelemetry());

        boolean isActive = "Active".equalsIgnoreCase(item.status) || "Resolving".equalsIgnoreCase(item.status);
        holder.btnAction.setText(isActive ? "||" : ">");
        holder.btnAction.setOnClickListener(v -> {
            if (listener != null) {
                listener.onTogglePause(item);
            }
        });

        holder.segmentBar.setSegments(item.segments);

        holder.layoutExpanded.setVisibility(item.isExpanded ? View.VISIBLE : View.GONE);
        holder.tvDestPath.setText(item.destPath);
        holder.tvShaBadge.setVisibility(item.sha256Verified ? View.VISIBLE : View.GONE);

        convertView.setOnClickListener(v -> {
            item.isExpanded = !item.isExpanded;
            holder.layoutExpanded.setVisibility(item.isExpanded ? View.VISIBLE : View.GONE);
        });

        return convertView;
    }

    static class ViewHolder {
        TextView tvMimeBadge;
        TextView tvTitle;
        TextView tvTelemetry;
        TextView btnAction;
        MultiSegmentProgressBar segmentBar;
        View layoutExpanded;
        TextView tvDestPath;
        TextView tvShaBadge;
    }
}
