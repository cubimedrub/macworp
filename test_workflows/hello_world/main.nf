#!/usr/bin/env nextflow

params.inFile = params.inFile ?: { log.error "No inFile file provided."; exit 1 }()
params.inFiles = params.inFiles ?: { log.error "No inFiles files provided."; exit 1 }()
params.inFolder = params.inFolder ?: { log.error "No inFolder file provided."; exit 1 }()
params.inFolders = params.inFolders ?: { log.error "No inFolders files provided."; exit 1 }()
params.numberOfNewlines = params.numberOfNewlines ?: { log.error "No numberOfNewlines provided."; exit 1 }()
params.singleLineText = params.singleLineText ?: { log.error "No singleLineText provided."; exit 1 }()
params.multilineText = params.multilineText ?: { log.error "No multilineText provided."; exit 1 }()
params.txtFiles = params.txtFiles ?: { log.error "No txtFiles provided."; exit 1 }()
params.outDir = "./"

/**
 * Use file from NF-Cloud PathSelector,
 * converts it to uppercase and
 * returns it.
 */
process readInFileAndConvertToUpperCase {
    input:
    path in_file

    output:
    path in_file_content

    """
    cat ${in_file} | tr '[a-z]' '[A-Z]' > in_file_content
    """
}

/**
 * Use file from NF-Cloud PathSelector,
 * converts it to uppercase and
 * returns it.
 */
process listInFolderContent {
    input:
    path in_folder

    output:
    path in_folder_content

    """
    ls -lah ${in_folder} > in_folder_content
    """
}

/**
 * Use comma separated files from NF-Cloud MultiplePathSelector,
 * write them as markdown list to a file
 * and returns file.
 */
process selectedFilesToMarkdownList {
    input:
    val in_files

    output:
    path selected_files_list

    """
    for file in \$(echo "${in_files}" | tr ',' ' ')
    do 
        echo "* \$file" >> selected_files_list
    done
    """
}

/**
 * Use comma separated folder from NF-Cloud MultiplePathSelector,
 * write them as markdown list to a file
 * and returns file.
 */
process selectedFoldersToMarkdownList {
    input:
    val in_files

    output:
    path selected_folder_list

    """
    for file in \$(echo "${in_files}" | tr ',' ' ')
    do 
        echo "* \$file" >> selected_folder_list
    done
    """
}

/**
 * Reads the collected txt-files,
 * and write them as markdown code blocks
 * to separate files.
 */
process txtFilesToMarkdownCodeBlocks {
    input:
    path txt_file

    output:
    path "codeblock_${txt_file.baseName}"

    """
    echo "${txt_file}" > codeblock_${txt_file.baseName}
    echo '```' >> codeblock_${txt_file.baseName}
    cat ${txt_file} >> codeblock_${txt_file.baseName}
    echo '```' >> codeblock_${txt_file.baseName}
    """
}

/**
 * Downloads test images for image viewer and SVG viewer
 */
process downloadTestImages {
    publishDir "${params.outDir}", mode:"copy"

    output:
    path "*.png"
    path "*.svg"

    """
    curl -o test_image.png  https://upload.wikimedia.org/wikipedia/commons/c/c4/PM5544_with_non-PAL_signals.png
    curl -o another_test_image.png  https://upload.wikimedia.org/wikipedia/commons/f/f1/SWTestbild.png
    curl -o test_svg.svg  https://upload.wikimedia.org/wikipedia/commons/0/02/SVG_logo.svg
    """
}

/**
 * Downloads test svg for image viewer
 */
process downloadTestPDF {
    publishDir "${params.outDir}", mode:"copy"

    output:
    path "*.pdf"

    """
    curl -L -o test_pdf.pdf  https://raw.githubusercontent.com/pdf-association/pdf20examples/master/Simple%20PDF%202.0%20file.pdf
    """
}


/**
 * Put all things together in one markdown file
 * and append NF-Cloud single- & multline TextInput.
 * The resulting outfile will be added to the workdir.
 */
process createMarkdownFile {

    publishDir "${params.outDir}", mode:"copy"

    input:
    path in_file_content
    path selected_files_list
    val txt_files
    path selected_folder_list
    path in_folder_content

    output:
    path "out.md"

    """
    out_md="out.md"

    echo "# Test workflow out.md" >> \$out_md

    echo "## Arguments" >> \$out_md
    echo '* params.inFile: `${params.inFile}`' >> \$out_md
    echo '* params.inFolder: `${params.inFolder}`' >> \$out_md
    echo '* params.inFiles: `${params.inFiles}`' >> \$out_md
    echo '* params.inFolders: `${params.inFolders}`' >> \$out_md
    echo '* params.numberOfNewlines: `${params.numberOfNewlines}`' >> \$out_md
    echo '* params.singleLineText: `${params.singleLineText}`' >> \$out_md
    # sed is for escaping the newlines.
    echo '* params.multilineText: `${params.multilineText.replace("\n", "\\n")}`' >> \$out_md
    echo '* params.txtFiles: `${params.txtFiles}`' >> \$out_md
    echo "" >> \$out_md
    
    echo "## Uppercase content of inFile" >> \$out_md
    cat "${in_file_content}" >> \$out_md
    echo "" >> \$out_md

    echo "## List of inFiles" >> \$out_md
    cat ${selected_files_list} >> \$out_md
    echo "" >> \$out_md

    echo "## inFolder content" >> \$out_md
    cat "${in_folder_content}" >> \$out_md
    echo "" >> \$out_md

    echo "## List of inFolders" >> \$out_md
    cat ${selected_folder_list} >> \$out_md
    echo "" >> \$out_md

    echo "## FileGlob selected file names and content" >> \$out_md
    text_files=(${txt_files.join(' ')})
    for txt_file in \${text_files[@]}
    do
        cat \$txt_file >> \$out_md
    done
    echo "" >> \$out_md

    echo "## Number of newlines following this section" >> \$out_md
    echo "${params.numberOfNewlines}" >> \$out_md
    for i in {1..${params.numberOfNewlines}}
    do 
        echo "" >> \$out_md
    done

    echo "## singleLineText argument" >> \$out_md
    echo "${params.singleLineText}" >> \$out_md
    echo "" >> \$out_md

    echo "## multilineText argument" >> \$out_md
    echo "${params.multilineText}" >> \$out_md
    echo "" >> \$out_md
    """
}

workflow {
    /**
     * Uses the value of NF-Cloud FileGlob to collect txt files with wildcard in path.
     */
    text_files = Channel.fromPath(params.txtFiles)
    inFileContent = readInFileAndConvertToUpperCase(Channel.fromPath(params.inFile))
    inFolderContent = listInFolderContent(Channel.fromPath(params.inFolder))
    selectedFilesList = selectedFilesToMarkdownList(Channel.fromPath(params.inFiles))
    selectedFoldersList = selectedFoldersToMarkdownList(Channel.fromPath(params.inFolders))
    txtFilesCodeBlock = txtFilesToMarkdownCodeBlocks(text_files)
    downloadTestImages()
    downloadTestPDF()
    createMarkdownFile(
            inFileContent,
            selectedFilesList,
            txtFilesCodeBlock.collect(),
            selectedFoldersList,
            inFolderContent
    )
}