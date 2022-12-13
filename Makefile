NF_HOSTNAME=$$(hostname)
USER_ID=$$(id -u)
GROUP_ID=$$(id -g)

worker:
	# Worker for native development
	env PYTHONUNBUFFERED=1 conda run --no-capture-output --live-stream -n nf_cloud \
		python -m nf_cloud_worker -n ./nextflow -c http://localhost:3001 -r amqp://admin:developer@127.0.0.1:5674/%2f -q project_workflow -d ./uploads -w ./nf_cloud.local.config.yaml -u worker -p developer

dev:
	# Back- & frontend for nativ development
	env OAUTHLIB_INSECURE_TRANSPORT=1 conda run --no-capture-output --live-stream -n nf_cloud honcho -e dev.env start

production-test:
	# Production test
	# Create separate upload directory
	mkdir -p deploy_test_uploads/
	# Build docker container for back- & frontend
	env DOCKER_BUILDKIT=1 docker build --target development -t "nf-cloud/backend:dev" --build-arg USER_ID=${USER_ID} --build-arg GROUP_ID=${GROUP_ID} -f backend.dockerfile .
	env DOCKER_BUILDKIT=1 docker build -t "nf-cloud/frontend:dev" -f frontend.dockerfile .
	# Start docker-compose in separate docker-compose project called nf-cloud-deploy-test, combining the two docker-compose files
	env NF_HOSTNAME=${NF_HOSTNAME} USER_ID=${USER_ID} GROUP_ID=${GROUP_ID} NF_FUSIONAUTH_PROTOCOL=https NF_FUSIONAUTH_PORT=16161 \
		docker-compose -p nf-cloud-deploy-test -f docker-compose.yaml -f deploy-test.docker-compose.yaml up ${args}

production-worker-test:
	# Worker for production test
	env PYTHONUNBUFFERED=1 conda run --no-capture-output --live-stream -n nf_cloud \
		python -m nf_cloud_worker -n ./nextflow -c https://winkelhardtdi-server:16160 -r amqp://admin:developer@127.0.0.1:5674/%2f -q project_workflow -d ./deploy_test_uploads -w ./config.docker-compose.yaml -u worker -p developer