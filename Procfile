frontend: cd frontend && yarn dev
backend: python -m macworp_backend serve
# Worker needs to run outside of honcho otherwise SDKMAN is not properly initialized when running Nextflow
# worker: env PYTHONUNBUFFERED=1 python -m macworp_worker -n ./nextflow -s $(which snakemake) -c http://localhost:3001 -r amqp://admin:developer@127.0.0.1:5674/%2f -q project_workflow -d ./uploads -u worker -p developer -vvvvvvvv
