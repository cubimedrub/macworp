use crate::schema_validation::{PathChecker, SchemaValidator, ValidationIssue};

use super::{CellKind, Column, Sheet};

impl Sheet {
    /// Cell types per sheet column, taken from the schema columns (unknown columns are text).
    ///
    /// # Arguments
    /// * `columns` - columns derived from the schema
    pub fn kinds(&self, columns: &[Column]) -> Vec<CellKind> {
        self.columns
            .iter()
            .map(|name| {
                columns
                    .iter()
                    .find(|column| &column.name == name)
                    .map(Column::cell_kind)
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Validates the sheet against its schema. Issues carry the row (0-based, header excluded)
    /// and column of the offending cell where the schema tells them.
    ///
    /// # Arguments
    /// * `validator` - validator built from the sample sheet schema
    /// * `columns` - columns derived from the same schema (for cell typing)
    /// * `paths` - answers whether referenced project paths exist
    pub fn validate(
        &self,
        validator: &SchemaValidator,
        columns: &[Column],
        paths: &dyn PathChecker,
    ) -> Vec<ValidationIssue> {
        validator.validate(&self.to_value(&self.kinds(columns)), paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_sheet::Format;
    use crate::schema_validation::AssumeExisting;

    /// A missing required cell and a bad type are reported on their row and column.
    #[test]
    fn issues_point_at_cells() {
        let schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nf_core_demo_schema_input.json"
        ))
        .unwrap();
        let validator = SchemaValidator::new(&schema).unwrap();
        let columns = Column::from_schema(&schema);
        let good = Sheet::parse(Format::Csv, "sample,fastq_1\nA,a_R1.fastq.gz\n").unwrap();
        assert!(
            good.validate(&validator, &columns, &AssumeExisting)
                .is_empty()
        );
        let bad = Sheet::parse(Format::Csv, "sample,fastq_1\nA,a.txt\nB c,b.fq.gz\n").unwrap();
        let issues = bad.validate(&validator, &columns, &AssumeExisting);
        assert!(
            issues
                .iter()
                .any(|i| i.row == Some(0) && i.column.as_deref() == Some("fastq_1"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.row == Some(1) && i.column.as_deref() == Some("sample"))
        );
    }
}
