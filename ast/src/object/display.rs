use std::fmt::{Display, Formatter, Result};

use crate::parsed::ExpressionWithTypeScheme;

use super::{Link, LinkFrom, LinkTo, Location, Machine, Object, Operation, PILGraph};

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.limbs.join("_"))
    }
}

impl Display for PILGraph {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        writeln!(f, "// Utilities")?;
        for (name, ExpressionWithTypeScheme { e, type_scheme }) in &self.definitions {
            writeln!(
                f,
                "let{} {name}{} = {e};",
                type_scheme
                    .as_ref()
                    .map(|ts| ts.type_vars_to_string())
                    .unwrap_or_default(),
                type_scheme
                    .as_ref()
                    .map(|ts| format!(": {}", ts.type_name))
                    .unwrap_or_default()
            )?;
        }
        for (location, object) in &self.objects {
            writeln!(f, "// Object {}", location)?;
            writeln!(f, "{object}")?;
            writeln!(f)?;
        }
        Ok(())
    }
}

impl Display for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(degree) = self.degree {
            writeln!(f, "// Degree {}", degree)?;
        }
        for s in &self.pil {
            writeln!(f, "{s}")?;
        }
        if !self.links.is_empty() {
            writeln!(f, "// Links:")?;
            for link in &self.links {
                writeln!(f, "// {link}")?;
            }
        }
        Ok(())
    }
}

impl Display for Link {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{} links to {}", self.from, self.to)
    }
}

impl Display for LinkFrom {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{} {}", self.flag, self.params)
    }
}

impl Display for LinkTo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{} in {}", self.operation, self.machine)
    }
}

impl Display for Machine {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "object at location \"{}\" with latch \"{:?}\" and operation_id \"{:?}\"",
            self.location, self.latch, self.operation_id
        )
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "operation \"{}\" with id {:?} with params {}",
            self.name,
            self.id.as_ref().map(|id| id.to_string()),
            self.params,
        )
    }
}
