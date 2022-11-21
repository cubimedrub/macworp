frontend: cd nf_cloud_frontend && yarn dev
backend: python -m nf_cloud_backend serve
worker: env PYTHONUNBUFFERED=1 python -m nf_cloud_worker -n ./nextflow -c http://localhost:3001 -r amqp://admin:developer@127.0.0.1:5674/%2f -q project_workflow -d ./uploads -w ./nf_cloud.local.config.yaml -u worker -p developer