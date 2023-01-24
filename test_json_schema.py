from pathlib import Path
import json
import jsonschema
import yaml


schema = json.loads(Path("/home/winkelhardtdi/Code/python/nf-cloud/backend/src/nf_cloud_backend/json_schemes/workflow.schema.json").read_text())
seed_data = yaml.load(Path("/home/winkelhardtdi/Code/python/nf-cloud/backend/db_seed.yaml").read_text(), Loader=yaml.FullLoader)["seeds"]

for seed in seed_data:
    if seed["model"] == "Workflow":
        workflow_definition = json.loads(seed["attributes"]["definition"])
        jsonschema.validate(instance=workflow_definition, schema=schema)
