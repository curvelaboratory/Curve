import os
from jinja2 import Environment, FileSystemLoader

ENVOY_CONFIG_TEMPLATE_FILE = os.getenv('ENVOY_CONFIG_TEMPLATE_FILE', 'envoy.template.yaml')
BOLT_CONFIG_FILE = os.getenv('BOLT_CONFIG_FILE', 'bolt_config.yaml')
ENVOY_CONFIG_FILE_RENDERED = os.getenv('ENVOY_CONFIG_FILE_RENDERED', '/usr/src/app/out/envoy.yaml')

env = Environment(loader=FileSystemLoader('./'))
template = env.get_template('envoy.template.yaml')

with open(BOLT_CONFIG_FILE, 'r') as file:
    curvelaboratory_config = file.read()

data = {
    'curvelaboratory_config': curvelaboratory_config
}

rendered = template.render(data)
print(rendered)
print(ENVOY_CONFIG_FILE_RENDERED)
with open(ENVOY_CONFIG_FILE_RENDERED, 'w') as file:
    file.write(rendered)
