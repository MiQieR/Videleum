#!/bin/bash

# Sample Hook Script for Videleum
# Usage: Configure this script in Videleum Settings -> Advanced -> Task Hooks
# Example: Set "Task Success" to: bash /path/to/documents/sample_hook.sh

# Output log file
LOG_FILE="/tmp/videleum_hook_sample.log"

echo "==================================================" >> "$LOG_FILE"
echo "Videleum Hook Triggered at $(date)" >> "$LOG_FILE"
echo "==================================================" >> "$LOG_FILE"

# Log Basic Task Information
echo "Task ID:      $VIDELEUM_TASK_ID" >> "$LOG_FILE"
echo "Task Title:   $VIDELEUM_TASK_TITLE" >> "$LOG_FILE"
echo "Task Status:  $VIDELEUM_TASK_STATUS" >> "$LOG_FILE"
echo "Source URL:   $VIDELEUM_SOURCE_URL" >> "$LOG_FILE"

# Handle specific states
if [ "$VIDELEUM_TASK_STATUS" == "success" ]; then
    echo "✅ Download Completed Successfully" >> "$LOG_FILE"
    echo "   Video Path:     $VIDELEUM_VIDEO_PATH" >> "$LOG_FILE"
    echo "   Thumbnail Path: $VIDELEUM_THUMBNAIL_PATH" >> "$LOG_FILE"

    # Example: Check file existence and size
    if [ -f "$VIDELEUM_VIDEO_PATH" ]; then
        SIZE=$(ls -lh "$VIDELEUM_VIDEO_PATH" | awk '{print $5}')
        echo "   File Size:      $SIZE" >> "$LOG_FILE"
        echo "   File exists and is ready for post-processing." >> "$LOG_FILE"
    else
        echo "   ⚠️ Warning: Video file not found at path." >> "$LOG_FILE"
    fi

elif [ "$VIDELEUM_TASK_STATUS" == "fail" ]; then
    echo "❌ Download Failed" >> "$LOG_FILE"
    echo "   Error: $VIDELEUM_ERROR" >> "$LOG_FILE"

elif [ "$VIDELEUM_TASK_STATUS" == "cancelled" ]; then
    echo "⚠️ Download Cancelled by User" >> "$LOG_FILE"
fi

echo "--------------------------------------------------" >> "$LOG_FILE"
