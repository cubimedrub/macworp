#!/bin/bash

# Change to mamabauser after setting up the permissions and before executing the command


# Make the rest of the entrypoint arguments eval safe
cmd=( $@ )
printf -v cmd_eval_safe '%q ' "${cmd[@]}"

# Switch user
su $MAMBA_USER -c "$cmd_eval_safe"