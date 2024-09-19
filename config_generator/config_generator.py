import os
from jinja2 import Environment, FileSystemLoader
import yaml

ENVOY_CONFIG_TEMPLATE_FILE = os.getenv('ENVOY_CONFIG_TEMPLATE_FILE', 'envoy.template.yaml')
BOLT_CONFIG_FILE = os.getenv('BOLT_CONFIG_FILE', 'bolt_config.yaml')
ENVOY_CONFIG_FILE_RENDERED = os.getenv('ENVOY_CONFIG_FILE_RENDERED', '/usr/src/app/out/envoy.yaml')

env = Environment(loader=FileSystemLoader('./'))
template = env.get_template('envoy.template.yaml')

with open(BOLT_CONFIG_FILE, 'r') as file:
    curvelaboratory_config = file.read()

config_yaml = yaml.safe_load(curvelaboratory_config)

inferred_clusters = {}

for prompt_target in config_yaml["prompt_targets"]:
    cluster = prompt_target.get("endpoint", {}).get("cluster", "")
    if cluster not in inferred_clusters:
      inferred_clusters[cluster] = {
          "name": cluster,
          "address": cluster,
          "port": 80, # default port
      }

print(inferred_clusters)

clusters = config_yaml.get("clusters", {})

# override the inferred clusters with the ones defined in the config
for name, cluster in clusters.items():
    if name in inferred_clusters:
        print("updating cluster", cluster)
        inferred_clusters[name].update(cluster)
    else:
        inferred_clusters[name] = cluster

print("updated clusters", inferred_clusters)

data = {
    'curvelaboratory_config': curvelaboratory_config,
    'curve _clusters': inferred_clusters
}

rendered = template.render(data)
print(rendered)
print(ENVOY_CONFIG_FILE_RENDERED)
with open(ENVOY_CONFIG_FILE_RENDERED, 'w') as file:
    file.write(rendered)
