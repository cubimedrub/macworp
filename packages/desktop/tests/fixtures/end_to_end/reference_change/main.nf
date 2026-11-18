nextflow.enable.dsl = 2

// Reference change test workflow. Reads reference file `ref_a`, signals the test with
// `<project>/ready`, waits for `<project>/go` (the test changes the reference data in between),
// then reads `ref_b`. results/result.txt holds what both reads returned.

params.ref_a = ''
params.ref_b = ''

process read_both {
    publishDir 'results', mode: 'copy'

    output:
    path 'result.txt'

    script:
    """
    cat ${params.ref_a} > a.copy
    touch ${launchDir}/ready
    while [ ! -f ${launchDir}/go ]; do sleep 1; done
    cat ${params.ref_b} > b.copy
    echo "a=\$(cat a.copy) b=\$(cat b.copy)" > result.txt
    """
}

workflow {
    read_both()
}
