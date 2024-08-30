use crate::program::Program;
use cranelift_isle::{
    sema::{ExternalSig, ReturnKind, TermId, Type, TypeEnv, TypeId},
    trie_again::{Binding, BindingId, Constraint, RuleSet},
};

pub fn print_rule_set(prog: &Program, term_id: &TermId, rule_set: &RuleSet) {
    println!("term {{");
    println!("\tname = {}", prog.term_name(*term_id));

    // Bindings.
    let lookup_binding = |binding_id: BindingId| rule_set.bindings[binding_id.index()].clone();
    println!("\tbindings = [");
    for (i, binding) in rule_set.bindings.iter().enumerate() {
        let ty = binding_type(binding, *term_id, &prog, lookup_binding);
        println!(
            "\t\t{i}: {}\t{}",
            ty.display(&prog.tyenv),
            binding_string(binding, *term_id, &prog, lookup_binding),
        );
    }
    println!("\t]");

    // Rules.
    println!("\trules = [");
    for rule in &rule_set.rules {
        assert_eq!(rule.iterators.len(), 0);
        println!("\t\t{{");
        println!(
            "\t\t\tpos = {}",
            rule.pos.pretty_print_line(&prog.tyenv.filenames[..])
        );
        println!("\t\t\tconstraints = [");
        for i in 0..rule_set.bindings.len() {
            if let Some(constraint) = rule.get_constraint(i.try_into().unwrap()) {
                println!(
                    "\t\t\t\t{}:\t{}",
                    i,
                    constraint_string(&constraint, &prog.tyenv)
                );
            }
        }
        println!("\t\t\t]");
        if !rule.equals.is_empty() {
            println!("\t\t\tequals = [");
            for i in 0..rule_set.bindings.len() {
                let binding_id = i.try_into().unwrap();
                if let Some(eq) = rule.equals.find(binding_id) {
                    if eq != binding_id {
                        println!("\t\t\t\t{} == {}", binding_id.index(), eq.index());
                    }
                }
            }
            println!("\t\t\t]");
        }
        println!("\t\t\tprio = {}", rule.prio);
        println!("\t\t\tresult = {}", rule.result.index());
        if !rule.impure.is_empty() {
            println!(
                "\t\t\timpure = {impure:?}",
                impure = rule
                    .impure
                    .iter()
                    .copied()
                    .map(BindingId::index)
                    .collect::<Vec<_>>()
            );
        }
        println!("\t\t}}");
    }
    println!("\t]");

    println!("}}");
}

pub fn binding_string(
    binding: &Binding,
    term_id: TermId,
    prog: &Program,
    lookup_binding: impl Fn(BindingId) -> Binding,
) -> String {
    match binding {
        Binding::Argument { index } => format!("argument({})", index.index()),
        Binding::ConstInt { val, ty } => {
            let ty = &prog.tyenv.types[ty.index()];
            format!("const_int({val}, {name})", name = ty.name(&prog.tyenv))
        }
        Binding::ConstPrim { val } => format!("const_prim({})", prog.tyenv.syms[val.index()]),
        Binding::Constructor {
            term,
            parameters,
            instance,
        } => {
            let name = prog.term_name(*term);
            format!(
                "constructor({name}, {parameters:?}, {instance})",
                parameters = parameters
                    .iter()
                    .copied()
                    .map(BindingId::index)
                    .collect::<Vec<_>>()
            )
        }
        Binding::Extractor { term, parameter } => {
            let name = prog.term_name(*term);
            format!(
                "extractor({name}, {parameter})",
                parameter = parameter.index()
            )
        }
        Binding::MatchVariant {
            source,
            variant,
            field,
        } => {
            let source_binding = lookup_binding(*source);
            let source_type = binding_type(&source_binding, term_id, prog, lookup_binding);
            let source_type_id = match source_type {
                BindingType::Base(type_id) => type_id,
                _ => unreachable!("source of match variant should be a base type"),
            };

            // Lookup variant.
            let enum_ty = &prog.tyenv.types[source_type_id.index()];
            let enum_name = enum_ty.name(&prog.tyenv);
            let variant = match enum_ty {
                Type::Enum { variants, .. } => &variants[variant.index()],
                _ => unreachable!("source match variant should be an enum"),
            };
            let variant_name = &prog.tyenv.syms[variant.name.index()];

            // Field.
            let field = &variant.fields[field.index()];
            let field_name = &prog.tyenv.syms[field.name.index()];

            format!(
                "match_variant({source}, {enum_name}::{variant_name}, {field_name})",
                source = source.index(),
            )
        }
        Binding::MakeVariant {
            ty,
            variant,
            fields,
        } => {
            let ty = &prog.tyenv.types[ty.index()];
            let variant = match ty {
                Type::Enum { variants, .. } => &variants[variant.index()],
                _ => unreachable!("source match variant should be an enum"),
            };
            let variant_name = &prog.tyenv.syms[variant.name.index()];
            format!(
                "make_variant({ty}::{variant_name}, {fields:?})",
                ty = ty.name(&prog.tyenv),
                fields = fields
                    .iter()
                    .copied()
                    .map(BindingId::index)
                    .collect::<Vec<_>>()
            )
        }
        Binding::MatchSome { source } => format!("match_some({source})", source = source.index()),
        Binding::MatchTuple { source, field } => format!(
            "match_tuple({source}, {field})",
            source = source.index(),
            field = field.index()
        ),
        _ => todo!("binding: {binding:?}"),
    }
}

pub fn constraint_string(constraint: &Constraint, tyenv: &TypeEnv) -> String {
    match constraint {
        Constraint::Variant { ty, variant, .. } => {
            let ty = &tyenv.types[ty.index()];
            match ty {
                Type::Primitive(_, sym, _) => {
                    format!("variant({})", tyenv.syms[sym.index()].clone())
                }
                Type::Enum { name, variants, .. } => {
                    let name = &tyenv.syms[name.index()];
                    let variant = &variants[variant.index()];
                    let variant_name = &tyenv.syms[variant.name.index()];
                    format!("variant({name}::{variant_name})")
                }
            }
        }
        Constraint::ConstInt { val, .. } => format!("const_int({})", val),
        Constraint::ConstPrim { val } => format!("const_prim({})", tyenv.syms[val.index()]),
        Constraint::Some => "some".to_string(),
    }
}

#[derive(Clone, Debug)]
pub enum BindingType {
    Base(TypeId),
    Option(Box<BindingType>),
    Tuple(Vec<BindingType>),
}

impl BindingType {
    pub fn display(&self, tyenv: &TypeEnv) -> String {
        match self {
            BindingType::Base(type_id) => {
                let ty = &tyenv.types[type_id.index()];
                ty.name(tyenv).to_string()
            }
            BindingType::Option(inner) => format!("Option({})", inner.display(tyenv)),
            BindingType::Tuple(inners) => format!(
                "({inners})",
                inners = inners
                    .iter()
                    .map(|inner| inner.display(tyenv))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

// Determine the type of a given binding.
pub fn binding_type(
    binding: &Binding,
    term_id: TermId,
    prog: &Program,
    lookup_binding: impl Fn(BindingId) -> Binding,
) -> BindingType {
    match binding {
        Binding::ConstInt { ty, .. } | Binding::MakeVariant { ty, .. } => BindingType::Base(*ty),

        Binding::ConstPrim { val } => BindingType::Base(prog.tyenv.const_types[val]),

        Binding::Argument { index } => {
            let term = &prog.termenv.terms[term_id.index()];
            BindingType::Base(term.arg_tys[index.index()])
        }

        Binding::Extractor { term, .. } => {
            // Determine the extractor signature.
            let term = &prog.termenv.terms[term.index()];
            let sig = term
                .extractor_sig(&prog.tyenv)
                .expect("term should have extractor signature");
            external_sig_return_type(&sig)
        }

        Binding::Constructor { term, .. } => {
            // Determine the constructor signature.
            let term = &prog.termenv.terms[term.index()];
            let sig = term
                .constructor_sig(&prog.tyenv)
                .expect("term should have constructor signature");
            external_sig_return_type(&sig)
        }

        Binding::MatchSome { source } => {
            let source_binding = lookup_binding(*source);
            let source_ty = binding_type(&source_binding, term_id, prog, lookup_binding);
            match source_ty {
                BindingType::Option(ty) => *ty,
                _ => unreachable!("source of match some should be an option"),
            }
        }

        Binding::MatchVariant {
            source,
            variant,
            field,
        } => {
            // Lookup type ID for the underlying enum.
            let source_binding = lookup_binding(*source);
            let source_ty = binding_type(&source_binding, term_id, prog, lookup_binding);
            let source_type_id = match source_ty {
                BindingType::Base(type_id) => type_id,
                _ => unreachable!("source of match variant should be a base type"),
            };

            // Lookup variant.
            let enum_ty = &prog.tyenv.types[source_type_id.index()];
            let variant = match enum_ty {
                Type::Enum { variants, .. } => &variants[variant.index()],
                _ => unreachable!("source match variant should be an enum"),
            };

            // Lookup field type.
            let field = &variant.fields[field.index()];
            BindingType::Base(field.ty)
        }

        Binding::MatchTuple { source, field } => {
            let source_binding = lookup_binding(*source);
            let source_ty = binding_type(&source_binding, term_id, prog, lookup_binding);
            match source_ty {
                BindingType::Tuple(tys) => tys[field.index()].clone(),
                _ => unreachable!("source type should be a tuple"),
            }
        }

        Binding::Iterator { .. } => unimplemented!("iterator bindings not supported"),
    }
}

fn external_sig_return_type(sig: &ExternalSig) -> BindingType {
    // Multiple return types are represented as a tuple.
    let ty = if sig.ret_tys.len() == 1 {
        BindingType::Base(sig.ret_tys[0].clone())
    } else {
        BindingType::Tuple(
            sig.ret_tys
                .iter()
                .copied()
                .map(|type_id| BindingType::Base(type_id))
                .collect(),
        )
    };

    // Fallible terms return option type.
    match sig.ret_kind {
        ReturnKind::Option => BindingType::Option(Box::new(ty)),
        ReturnKind::Plain => ty,
        ReturnKind::Iterator => unimplemented!("extractor iterator return"),
    }
}
