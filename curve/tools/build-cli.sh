#!/bin/bash

# Define paths
source_schema="../curve_config_schema.yaml"
source_compose="../docker-compose.yaml"
source_stage_env="../stage.env"
destination_dir="config"

# Ensure the destination directory exists only if it doesn't already
if [ ! -d "$destination_dir" ]; then
    mkdir -p "$destination_dir"
    echo "Directory $destination_dir created."
fi

# Copy the files
cp "$source_schema" "$destination_dir/curve_config_schema.yaml"
cp "$source_compose" "$destination_dir/docker-compose.yaml"
cp "$source_stage_env" "$destination_dir/stage.env"

# Print success message
echo "Files copied successfully!"

echo "Building the cli"
pip install -e .
