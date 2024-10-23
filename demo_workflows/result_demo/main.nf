#!/usr/bin/env nextflow

nextflow.enable.dsl=2
nextflow.preview.output = true

params.resultsFolder = 'results'


process download {
    output:
    path 'healthexp.csv'

    """
    curl -o healthexp.csv https://raw.githubusercontent.com/mwaskom/seaborn-data/refs/heads/master/raw/healthexp.csv
    """
}

process generate_result_files {
    input:
    path healthexp

    output:
    path '*_healthexp_2021.*'

    """
    gen_result_files.py $healthexp
    """
}

workflow  {
    csv = download()
    files = generate_result_files(csv)

    publish:
    files >> params.resultsFolder
}

/**
 * Move the output files to the results folder
 */
output {
    mode 'move'
}