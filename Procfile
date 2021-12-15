frontend: cd nf_cloud_frontend && yarn dev
backend: python -m nf_cloud_backend
worker: python -m nf_cloud_worker -c http://localhost:3001 -r amqp://admin:developer@127.0.0.1:5673/%2f -q workflows -d ./uploads -w ./config.development.yaml -w ./config.local.yaml