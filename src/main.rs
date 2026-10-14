use dotnetdll::prelude::*;
use std::collections::{HashMap, HashSet};
use std::env;

const PTR: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct Ty {
    ai: usize,
    idx: TypeIndex,
}

struct Asm {
    file: String,
    res: Resolution<'static>,
}

struct World {
    asms: Vec<Asm>,
    index: HashMap<String, Ty>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Algo {
    CoreClr,
    Decl,
    SizeDesc,
    RefFirstDecl,
    RefFirstSizeDesc,
    SizeDescRefFirst,
    RefFirstSizeDescAlign,
    RefFirstSmallFirst,
    SmallFirst,
    AlignAsc,
    RefFirstAlignAsc,
}

const ALGOS: [Algo; 11] = [
    Algo::CoreClr,
    Algo::Decl,
    Algo::SizeDesc,
    Algo::RefFirstDecl,
    Algo::RefFirstSizeDesc,
    Algo::SizeDescRefFirst,
    Algo::RefFirstSizeDescAlign,
    Algo::RefFirstSmallFirst,
    Algo::SmallFirst,
    Algo::AlignAsc,
    Algo::RefFirstAlignAsc,
];

fn place_coreclr(fields: &[Fld], mut cum: usize) -> (Vec<(String, usize)>, usize) {
    let mut counts = [0usize; 4];
    for f in fields {
        if !f.byvalue {
            counts[log2_size(f.size)] += 1;
        }
    }

    let mut starts = [0usize; 4];
    for i in (0..4).rev() {
        if counts[i] == 0 {
            continue;
        }
        cum = align_up(cum, 1 << i);
        starts[i] = cum;
        cum += counts[i] << i;
    }

    let gc_count = fields.iter().filter(|f| f.isref).count();
    let gc_start = starts[3];
    if gc_count > 0 {
        starts[3] += gc_count * PTR;
    }

    let mut seen = [0usize; 4];
    let mut placed_gc = 0usize;
    let mut out = Vec::with_capacity(fields.len());
    for f in fields {
        if f.isref {
            out.push((f.name.clone(), gc_start + placed_gc * PTR));
            placed_gc += 1;
        } else if !f.byvalue {
            let i = log2_size(f.size);
            let o = starts[i] + seen[i] * f.size;
            out.push((f.name.clone(), o));
            seen[i] += 1;
        }
    }

    for f in fields {
        if f.byvalue {
            cum = align_up(cum, f.align.max(1));
            out.push((f.name.clone(), cum));
            cum += f.size;
        }
    }

    (out, cum)
}

fn by_small_first(fields: &[Fld], mut idx: Vec<usize>) -> Vec<usize> {
    let mut small = Vec::new();
    let mut large = Vec::new();
    for i in idx.drain(..) {
        if fields[i].size < 8 { small.push(i) } else { large.push(i) }
    }
    small.extend(large);
    small
}

struct Fld {
    name: String,
    size: usize,
    align: usize,
    gc: bool,
    isref: bool,
    byvalue: bool,
}

fn log2_size(s: usize) -> usize {
    if s < 2 {
        return 0;
    }
    let l = (usize::BITS - 1 - s.leading_zeros()) as usize;
    l.min(3)
}

fn full_name(td: &TypeDefinition, res: &Resolution) -> String {
    match td.encloser {
        Some(e) => format!("{}+{}", full_name(&res[e], res), td.name),
        None => match &td.namespace {
            Some(ns) if !ns.is_empty() => format!("{}.{}", ns, td.name),
            _ => td.name.to_string(),
        },
    }
}

fn align_up(v: usize, a: usize) -> usize {
    if a <= 1 { v } else { v.div_ceil(a) * a }
}

fn strip_arity(n: &str) -> &str {
    match n.rfind('`') {
        Some(i) => &n[..i],
        None => n,
    }
}

fn vtype_size(cum: usize, largest: usize, gc: bool) -> (usize, usize, bool) {
    let min_align = if cum > PTR {
        if gc { PTR } else { largest.max(1) }
    } else {
        let mut m = 1usize;
        while m < cum {
            m <<= 1;
        }
        m
    };
    let size = align_up(cum, min_align);
    let align = if min_align != cum.min(PTR) {
        min_align
    } else {
        size.min(PTR)
    };
    (size, align, gc)
}

fn bcl_value(name: &str, args: &[MemberType], w: &World, ai: usize, depth: usize) -> Option<(usize, usize, bool)> {
    match strip_arity(name) {
        "System.Nullable" => {
            let (s, a, g) = w.size(ai, args.first()?, depth + 1);
            Some((align_up(s + 1, a), a, g))
        }
        "System.Collections.Generic.KeyValuePair" if args.len() == 2 => {
            let (s0, a0, g0) = w.size(ai, &args[0], depth + 1);
            let (s1, a1, g1) = w.size(ai, &args[1], depth + 1);
            let cum = align_up(align_up(0, a0.max(1)) + s0, a1.max(1)) + s1;
            Some(vtype_size(cum, a0.max(a1), g0 || g1))
        }
        "System.DateTime" => Some((8, 8, false)),
        "System.TimeSpan" => Some((8, 8, false)),
        "System.Guid" => Some((16, 4, false)),
        "System.Decimal" => Some((16, 8, false)),
        "System.Int128" | "System.UInt128" => Some((16, 8, false)),
        "System.Half" => Some((2, 2, false)),
        "System.Runtime.InteropServices.GCHandle" => Some((8, 8, false)),
        _ => None,
    }
}

impl World {
    fn load(paths: &[String]) -> World {
        let mut asms: Vec<Asm> = Vec::new();
        let mut index: HashMap<String, Ty> = HashMap::new();
        for p in paths {
            let bytes: &'static [u8] = match std::fs::read(p) {
                Ok(v) => Box::leak(v.into_boxed_slice()),
                Err(e) => {
                    eprintln!("skip {p}: {e}");
                    continue;
                }
            };
            let lazy = ReadOptions {
                skip_method_bodies: true,
                lazy_method_bodies: true,
                lazy_method_signatures: true,
                lazy_property_signatures: true,
                lazy_attributes: true,
                ..Default::default()
            };
            let res = Resolution::parse(bytes, ReadOptions::default())
                .or_else(|_| Resolution::parse(bytes, lazy))
                .map_err(|e| format!("{e:?}"));
            let res = match res {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("skip {p}: {e}");
                    continue;
                }
            };
            let ai = asms.len();
            for (idx, td) in res.enumerate_type_definitions() {
                let n = full_name(td, &res);
                index.entry(n).or_insert(Ty { ai, idx });
            }
            asms.push(Asm {
                file: std::path::Path::new(p)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone()),
                res,
            });
        }
        World { asms, index }
    }

    fn td(&self, t: Ty) -> &TypeDefinition<'static> {
        &self.asms[t.ai].res[t.idx]
    }

    fn name_of(&self, t: Ty) -> String {
        full_name(self.td(t), &self.asms[t.ai].res)
    }

    fn type_label(&self, ai: usize, t: &MemberType, depth: usize) -> String {
        if depth > 8 {
            return "?".into();
        }
        match t {
            MemberType::TypeGeneric(g) => format!("!{g:?}"),
            MemberType::Base(b) => match &**b {
                BaseType::Boolean => "bool".into(),
                BaseType::Char => "char".into(),
                BaseType::Int8 => "int8".into(),
                BaseType::UInt8 => "uint8".into(),
                BaseType::Int16 => "int16".into(),
                BaseType::UInt16 => "uint16".into(),
                BaseType::Int32 => "int32".into(),
                BaseType::UInt32 => "uint32".into(),
                BaseType::Int64 => "int64".into(),
                BaseType::UInt64 => "uint64".into(),
                BaseType::Float32 => "float32".into(),
                BaseType::Float64 => "float64".into(),
                BaseType::IntPtr => "intptr".into(),
                BaseType::UIntPtr => "uintptr".into(),
                BaseType::Object => "object".into(),
                BaseType::String => "string".into(),
                BaseType::Array(inner, _) => {
                    format!("{}[]", self.type_label(ai, inner, depth + 1))
                }
                BaseType::ValuePointer(_, inner) => match inner {
                    Some(i) => format!("{}*", self.type_label(ai, i, depth + 1)),
                    None => "void*".into(),
                },
                BaseType::Vector(_, inner) => self.type_label(ai, inner, depth + 1),
                BaseType::FunctionPointer(_) => "fnptr".into(),
                BaseType::Type { value_kind, source } => {
                    let (n, args) = self.src_name_args(ai, source);
                    let vk = match value_kind {
                        Some(ValueKind::Class) => "class ",
                        Some(ValueKind::ValueType) => "struct ",
                        None => "",
                    };
                    if args.is_empty() {
                        format!("{vk}{n}")
                    } else {
                        let inner: Vec<String> = args
                            .iter()
                            .map(|a| self.type_label(ai, a, depth + 1))
                            .collect();
                        format!("{vk}{n}<{}>", inner.join(", "))
                    }
                }
            },
        }
    }

    fn src_name_args(&self, ai: usize, src: &TypeSource<MemberType>) -> (String, Vec<MemberType>) {
        match src {
            TypeSource::User(u) => (self.user_name(ai, u), Vec::new()),
            TypeSource::Generic { base, parameters } => {
                (self.user_name(ai, base), parameters.clone())
            }
        }
    }

    fn user_name(&self, ai: usize, u: &UserType) -> String {
        match u {
            UserType::Definition(idx) => self.name_of(Ty { ai, idx: *idx }),
            UserType::Reference(rt) => self.asms[ai].res[*rt].type_name(),
        }
    }

    fn resolve(&self, ai: usize, src: &TypeSource<MemberType>) -> Option<(Ty, Vec<MemberType>)> {
        let (u, args) = match src {
            TypeSource::User(u) => (u, Vec::new()),
            TypeSource::Generic { base, parameters } => (base, parameters.clone()),
        };
        match u {
            UserType::Definition(idx) => Some((Ty { ai, idx: *idx }, args)),
            UserType::Reference(rt) => {
                let name = self.asms[ai].res[*rt].type_name();
                self.index.get(&name).map(|t| (*t, args))
            }
        }
    }

    fn extends_of(&self, t: Ty) -> Option<Ty> {
        let ext = self.td(t).extends.as_ref()?;
        let (b, _) = self.resolve(t.ai, ext)?;
        Some(b)
    }

    fn is_enum(&self, t: Ty) -> bool {
        matches!(self.extends_of(t), Some(b) if self.name_of(b) == "System.Enum")
    }

    fn size(&self, ai: usize, t: &MemberType, depth: usize) -> (usize, usize, bool) {
        if depth > 32 {
            return (PTR, PTR, true);
        }
        let b = match t {
            MemberType::TypeGeneric(_) => return (PTR, PTR, true),
            MemberType::Base(b) => b,
        };
        match &**b {
            BaseType::Boolean | BaseType::Int8 | BaseType::UInt8 => (1, 1, false),
            BaseType::Char | BaseType::Int16 | BaseType::UInt16 => (2, 2, false),
            BaseType::Int32 | BaseType::UInt32 | BaseType::Float32 => (4, 4, false),
            BaseType::Int64 | BaseType::UInt64 | BaseType::Float64 => (8, 8, false),
            BaseType::IntPtr | BaseType::UIntPtr => (PTR, PTR, false),
            BaseType::Object
            | BaseType::String
            | BaseType::Array(..)
            | BaseType::ValuePointer(..)
            | BaseType::FunctionPointer(..) => (PTR, PTR, true),
            BaseType::Vector(_, inner) => self.size(ai, inner, depth + 1),
            BaseType::Type { value_kind, source } => {
                if !matches!(value_kind, Some(ValueKind::ValueType)) {
                    return (PTR, PTR, true);
                }
                let (name, args) = self.src_name_args(ai, source);
                if let Some(r) = bcl_value(&name, &args, self, ai, depth) {
                    return r;
                }
                match self.resolve(ai, source) {
                    Some((t, _)) => self.value_layout(t, depth),
                    None => (PTR, PTR, true),
                }
            }
        }
    }

    fn value_layout(&self, t: Ty, depth: usize) -> (usize, usize, bool) {
        if depth > 32 {
            return (PTR, PTR, true);
        }
        let td = self.td(t);
        if self.is_enum(t) {
            if let Some(f) = td.fields.iter().find(|f| &*f.name == "value__") {
                let (s, _, _) = self.size(t.ai, &f.return_type, depth + 1);
                return (s, s, false);
            }
            return (4, 4, false);
        }

        let fields = self.own_fields(t, depth);
        let mut largest = 1usize;
        let mut gc = false;
        for f in &fields {
            if f.isref {
                largest = largest.max(PTR);
                gc = true;
            } else if f.byvalue {
                largest = largest.max(f.align.max(1));
                gc |= f.gc;
            } else {
                largest = largest.max(f.size.max(1));
            }
        }

        if !matches!(td.flags.layout, Layout::Automatic) && !gc {
            let mut off = 0usize;
            let mut maxa = 1usize;
            for f in &fields {
                let a = if f.byvalue {
                    f.align.max(1)
                } else if f.isref {
                    PTR
                } else {
                    f.size.max(1)
                };
                off = align_up(off, a) + f.size;
                maxa = maxa.max(a);
            }
            return (align_up(off, maxa), maxa, gc);
        }

        let (_, cum) = place_coreclr(&fields, 0);
        let min_align = if cum > PTR {
            if gc {
                PTR
            } else {
                largest
            }
        } else {
            let mut m = 1usize;
            while m < cum {
                m <<= 1;
            }
            m
        }
        .max(1);
        let size = align_up(cum, min_align);
        let align = if min_align != cum.min(PTR) {
            min_align
        } else {
            size.min(PTR)
        };
        (size, align, gc)
    }

    fn all_fields(&self, t: Ty, depth: usize) -> Vec<Fld> {
        if depth > 32 {
            return Vec::new();
        }
        let mut out = match self.extends_of(t) {
            Some(b) if self.name_of(b) != "System.Object" => self.all_fields(b, depth + 1),
            _ => Vec::new(),
        };
        out.extend(self.own_fields(t, depth));
        out
    }

    fn own_fields(&self, t: Ty, depth: usize) -> Vec<Fld> {
        self.td(t)
            .fields
            .iter()
            .filter(|f| !f.static_member && !f.literal)
            .map(|f| {
                let (s, a, g) = self.size(t.ai, &f.return_type, depth + 1);
                let (isref, byvalue) = self.field_kind(t.ai, &f.return_type, depth);
                Fld {
                    name: f.name.to_string(),
                    size: s,
                    align: a,
                    gc: g,
                    isref,
                    byvalue,
                }
            })
            .collect()
    }

    fn field_kind(&self, ai: usize, t: &MemberType, depth: usize) -> (bool, bool) {
        if depth > 32 {
            return (true, false);
        }
        let b = match t {
            MemberType::TypeGeneric(_) => return (true, false),
            MemberType::Base(b) => b,
        };
        match &**b {
            BaseType::Type { value_kind, source } => {
                if !matches!(value_kind, Some(ValueKind::ValueType)) {
                    return (true, false);
                }
                match self.resolve(ai, source) {
                    Some((ty, _)) => (false, !self.is_enum(ty)),
                    None => (false, true),
                }
            }
            BaseType::Vector(_, inner) => self.field_kind(ai, inner, depth + 1),
            _ => (false, false),
        }
    }

    fn layout(&self, t: Ty, algo: Algo, depth: usize, pack: bool) -> (Vec<(String, usize)>, usize) {
        if depth > 32 {
            return (Vec::new(), PTR);
        }
        let (mut out, base_size) = match self.extends_of(t) {
            Some(b) if self.name_of(b) != "System.Object" => self.layout(b, algo, depth + 1, pack),
            _ => (Vec::new(), PTR),
        };
        let own = self.own_fields(t, depth);
        let (placed, size) = place(&own, algo, base_size, pack);
        out.extend(placed);
        (out, size)
    }
}

fn place(fields: &[Fld], algo: Algo, start: usize, pack: bool) -> (Vec<(String, usize)>, usize) {
    if algo == Algo::CoreClr {
        let (out, cum) = place_coreclr(fields, start);
        return (out, align_up(cum, PTR));
    }
    let mut idx: Vec<usize> = (0..fields.len()).collect();
    match algo {
        Algo::CoreClr => {}
        Algo::Decl => {}
        Algo::SizeDesc => idx.sort_by_key(|&i| std::cmp::Reverse(fields[i].size)),
        Algo::SizeDescRefFirst => idx.sort_by_key(|&i| {
            (
                std::cmp::Reverse(fields[i].gc),
                std::cmp::Reverse(fields[i].size),
            )
        }),
        Algo::SmallFirst => idx = by_small_first(fields, idx),
        Algo::AlignAsc => idx.sort_by_key(|&i| fields[i].align),
        Algo::RefFirstDecl
        | Algo::RefFirstSizeDesc
        | Algo::RefFirstSizeDescAlign
        | Algo::RefFirstSmallFirst
        | Algo::RefFirstAlignAsc => {
            let mut r = Vec::new();
            let mut n = Vec::new();
            for i in 0..fields.len() {
                if fields[i].gc { r.push(i) } else { n.push(i) }
            }
            match algo {
                Algo::RefFirstSizeDesc => n.sort_by_key(|&i| std::cmp::Reverse(fields[i].size)),
                Algo::RefFirstSizeDescAlign => n.sort_by_key(|&i| {
                    (
                        std::cmp::Reverse(fields[i].size),
                        std::cmp::Reverse(fields[i].align),
                    )
                }),
                Algo::RefFirstSmallFirst => n = by_small_first(fields, n),
                Algo::RefFirstAlignAsc => n.sort_by_key(|&i| fields[i].align),
                _ => {}
            }
            r.extend(n);
            idx = r;
        }
    }
    let mut off = start;
    let mut out = Vec::with_capacity(fields.len());
    for i in idx {
        let f = &fields[i];
        off = align_up(off, f.align);
        out.push((f.name.clone(), off));
        off += f.size;
    }
    (out, if pack { align_up(off, PTR) } else { off })
}

fn split_type_field(w: &World, path: &str) -> Option<(String, String)> {
    let b = path.as_bytes();
    for i in (0..path.len()).rev() {
        if b[i] == b'.' {
            let (t, f) = (&path[..i], &path[i + 1..]);
            if let Some(ty) = w.index.get(t) {
                if w.all_fields(*ty, 0).iter().any(|x| &x.name == f) {
                    return Some((t.to_string(), f.to_string()));
                }
            }
        }
    }
    None
}

fn parse_offset(s: &str) -> Option<usize> {
    let tok = s.split_whitespace().next()?;
    usize::from_str_radix(tok.strip_prefix("0x")?, 16).ok()
}

fn dump(w: &World, name: &str, pack: bool) {
    let Some(t) = w.index.get(name).copied() else {
        println!("unknown type {name}");
        return;
    };
    let td = w.td(t);
    println!("type    : {name}");
    println!("layout  : {:?}", td.flags.layout);
    println!("kind    : {:?}", td.flags.kind);
    println!(
        "extends : {}",
        td.extends
            .as_ref()
            .map(|e| w.src_name_args(t.ai, e).0)
            .unwrap_or_else(|| "<none>".into())
    );
    println!("declared fields (declaration order, non-static, non-literal):");
    for f in td.fields.iter().filter(|f| !f.static_member && !f.literal) {
        let (s, a, g) = w.size(t.ai, &f.return_type, 0);
        println!(
            "   size={s:<3} align={a:<2} gc={g:<5} {:<28} {}",
            w.type_label(t.ai, &f.return_type, 0),
            f.name
        );
    }
    for a in ALGOS {
        let (placed, sz) = w.layout(t, a, 0, pack);
        println!("\n== {a:?} size=0x{sz:X} ==");
        for (n, o) in &placed {
            println!("   0x{o:04X}  {n}");
        }
    }
}

fn verify(w: &World, path: &str, pack: bool) {
    let text = std::fs::read_to_string(path).expect("read reference");
    let mut entries: Vec<(String, String, usize)> = Vec::new();
    let mut skipped = 0usize;
    let mut versions: HashSet<String> = HashSet::new();

    for line in text.lines() {
        let Some(i) = line.find("layouts[") else {
            skipped += 1;
            continue;
        };
        let rest = &line[i..];
        let Some(end) = rest.find(']') else { continue };
        versions.insert(rest[8..end].to_string());
        let tail = rest[end + 1..].trim_start_matches('.');
        let Some(eq) = tail.find(" = ") else { continue };
        let Some(off) = parse_offset(&tail[eq + 3..]) else {
            continue;
        };
        entries.push((tail[..eq].to_string(), rest[8..end].to_string(), off));
    }

    println!("reference entries: {}", entries.len());
    println!("versions: {:?}", versions);
    println!("non-layout lines: {skipped}");

    let mut resolved = Vec::new();
    let mut unresolved = Vec::new();
    for (p, v, o) in &entries {
        match split_type_field(w, p) {
            Some((t, f)) => resolved.push((t, f, *o, v.clone(), p.clone())),
            None => unresolved.push(p.clone()),
        }
    }
    println!("resolved {} / unresolved {}", resolved.len(), unresolved.len());
    for p in unresolved.iter().take(15) {
        println!("   unresolved: {p}");
    }

    let mut types: HashMap<String, Ty> = HashMap::new();
    for (t, ..) in &resolved {
        if let Some(x) = w.index.get(t) {
            types.insert(t.clone(), *x);
        }
    }

    let mut tables: HashMap<Algo, HashMap<String, Vec<(String, usize)>>> = HashMap::new();
    for a in ALGOS {
        let mut m = HashMap::new();
        for (t, ty) in &types {
            m.insert(t.clone(), w.layout(*ty, a, 0, pack).0);
        }
        tables.insert(a, m);
    }

    let total = resolved.len();

    let mut align_bad: Vec<String> = Vec::new();
    let mut overlap_bad: Vec<String> = Vec::new();
    let mut beyond_bad: Vec<String> = Vec::new();
    let mut fits = 0usize;
    let mut sized = 0usize;

    for (tname, ty) in &types {
        let mut ents: Vec<(String, usize)> = resolved
            .iter()
            .filter(|(t, ..)| t == tname)
            .map(|(_, f, o, ..)| (f.clone(), *o))
            .collect();
        ents.sort_by_key(|(_, o)| *o);
        let allf = w.all_fields(*ty, 0);
        let csz = w.layout(*ty, Algo::RefFirstSizeDesc, 0, pack).1;
        for (fname, off) in &ents {
            let Some(f) = allf.iter().find(|x| &x.name == fname) else {
                continue;
            };
            sized += 1;
            if f.align > 1 && off % f.align != 0 {
                align_bad.push(format!(
                    "{tname}.{fname}: 0x{off:X} not aligned to {}",
                    f.align
                ));
            }
            if off + f.size > csz {
                beyond_bad.push(format!(
                    "{tname}.{fname}: 0x{off:X}+{} beyond computed size 0x{csz:X}",
                    f.size
                ));
            } else {
                fits += 1;
            }
        }
        for p in ents.windows(2) {
            let (f0, o0) = &p[0];
            let (f1, o1) = &p[1];
            if let Some(a) = allf.iter().find(|x| &x.name == f0) {
                if o0 + a.size > *o1 {
                    overlap_bad.push(format!(
                        "{tname}: {f0} 0x{o0:X}+{} overlaps {f1} 0x{o1:X}",
                        a.size
                    ));
                }
            }
        }
    }

    println!("\nreport offsets with a known size: {sized}");
    println!("  fits inside computed size : {fits}");
    println!("  misaligned (impossible)   : {}", align_bad.len());
    for l in align_bad.iter().take(12) {
        println!("     {l}");
    }
    println!("  overlapping (impossible)  : {}", overlap_bad.len());
    for l in overlap_bad.iter().take(12) {
        println!("     {l}");
    }
    println!("  beyond computed size      : {}", beyond_bad.len());
    for l in beyond_bad.iter().take(12) {
        println!("     {l}");
    }

    let mut best = (0usize, Algo::Decl);
    for a in ALGOS {
        let mut hit = 0usize;
        for (t, f, o, ..) in &resolved {
            let got = tables[&a]
                .get(t)
                .and_then(|v| v.iter().find(|(n, _)| n == f))
                .map(|(_, x)| *x);
            if got == Some(*o) {
                hit += 1;
            }
        }
        if hit > best.0 {
            best = (hit, a);
        }
        println!(
            "  {a:?}: {hit}/{total} ({:.1}%)",
            100.0 * hit as f64 / total.max(1) as f64
        );
    }

    let a = best.1;
    println!("\nbest: {a:?} {}/{total}", best.0);

    let mut rows: Vec<(String, usize, usize, i64)> = Vec::new();
    let mut hist: HashMap<i64, usize> = HashMap::new();
    for (t, _ty) in &types {
        let mut ents: Vec<(&String, usize)> = resolved
            .iter()
            .filter(|(tt, ..)| tt == t)
            .map(|(_, f, o, ..)| (f, *o))
            .collect();
        if ents.is_empty() {
            continue;
        }
        ents.sort_by_key(|(_, o)| *o);
        let table = &tables[&a][t];
        let mut n = 0usize;
        let mut m = 0usize;
        let mut delta = 0i64;
        for (f, o) in &ents {
            n += 1;
            if let Some((_, g)) = table.iter().find(|(x, _)| x == *f) {
                if g == o {
                    m += 1;
                }
                if n == 1 {
                    delta = *o as i64 - *g as i64;
                }
            }
        }
        if m < n {
            *hist.entry(delta).or_insert(0) += 1;
            rows.push((t.clone(), m, n, delta));
        }
    }
    println!("\nordering-only score (types whose first-field delta is 0):");
    for a2 in ALGOS {
        let mut m = 0usize;
        let mut n = 0usize;
        let mut nt = 0usize;
        for (t, ty) in &types {
            let mut ents: Vec<(&String, usize)> = resolved
                .iter()
                .filter(|(tt, ..)| tt == t)
                .map(|(_, f, o, ..)| (f, *o))
                .collect();
            if ents.is_empty() {
                continue;
            }
            ents.sort_by_key(|(_, o)| *o);
            let table = &tables[&a2][t];
            let mut delta = None;
            for (f, o) in &ents {
                if let Some((_, g)) = table.iter().find(|(x, _)| x == *f) {
                    delta = Some(*o as i64 - *g as i64);
                    break;
                }
            }
            let base_is_object = match w.extends_of(*ty) {
                Some(b) => w.name_of(b) == "System.Object",
                None => true,
            };
            if !base_is_object {
                continue;
            }
            let _ = delta;
            if false {
                continue;
            }
            nt += 1;
            for (f, o) in &ents {
                n += 1;
                if let Some((_, g)) = table.iter().find(|(x, _)| x == *f) {
                    if g == o {
                        m += 1;
                    }
                }
            }
        }
        println!(
            "   {a2:?}: {m}/{n} ({:.1}% of {nt} types)",
            100.0 * m as f64 / n.max(1) as f64
        );
    }

    println!("\ntypes with a nonzero delta, shortest base chains first:");
    let mut rows: Vec<(i64, usize, String)> = Vec::new();
    for (t, ty) in &types {
        let mut ents: Vec<(&String, usize)> = resolved
            .iter()
            .filter(|(tt, ..)| tt == t)
            .map(|(_, f, o, ..)| (f, *o))
            .collect();
        if ents.is_empty() {
            continue;
        }
        ents.sort_by_key(|(_, o)| *o);
        let table = &tables[&Algo::CoreClr][t];
        let mut delta = 0i64;
        for (f, o) in &ents {
            if let Some((_, g)) = table.iter().find(|(x, _)| x == *f) {
                delta = *o as i64 - *g as i64;
                break;
            }
        }
        if delta == 0 {
            continue;
        }
        let mut chain: Vec<String> = Vec::new();
        let mut cur = w.extends_of(*ty);
        while let Some(c) = cur {
            chain.push(w.name_of(c).to_string());
            if chain.len() >= 7 {
                break;
            }
            cur = w.extends_of(c);
        }
        rows.push((
            delta.abs(),
            chain.len(),
            format!("{delta:+5} depth={:<2} {t} <- {}", chain.len(), chain.join(" <- ")),
        ));
    }
    rows.sort();
    for (_, _, line) in rows.iter().take(45) {
        println!("   {line}");
    }

    let mut hs: Vec<(i64, usize)> = hist.into_iter().collect();
    hs.sort_by_key(|(d, _)| *d);
    println!("\nper-type first-field delta histogram (delta, #types):");
    for (d, c) in &hs {
        println!("   {d:>6}  {c}");
    }
    let mut shown = 0usize;
    for (t, f, o, v, p) in &resolved {
        let got = tables[&a]
            .get(t)
            .and_then(|x| x.iter().find(|(n, _)| n == f))
            .map(|(_, x)| *x);
        if got != Some(*o) {
            if shown < 60 {
                let g = match got {
                    Some(g) => format!("0x{g:X}"),
                    None => "absent".into(),
                };
                println!("   MISMATCH {v} {t}.{f}: md=0x{o:X} md.got={g}");
                let _ = p;
            }
            shown += 1;
        }
    }
    println!("total mismatches: {shown}");
}

fn ident(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if c.is_ascii_graphic() {
            s.push('_');
        } else {
            s.push_str(&format!("u{:04x}", c as u32));
        }
    }
    if s.chars().all(|c| c == '_') {
        s.insert(0, 't');
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

struct TypeOut {
    name: String,
    parent: String,
    fields: Vec<(String, usize)>,
}

fn constant_value(c: &Constant) -> Option<i64> {
    Some(match c {
        Constant::Int8(v) => *v as i64,
        Constant::UInt8(v) => *v as i64,
        Constant::Int16(v) => *v as i64,
        Constant::UInt16(v) => *v as i64,
        Constant::Int32(v) => *v as i64,
        Constant::UInt32(v) => *v as i64,
        Constant::Int64(v) => *v,
        Constant::UInt64(v) => *v as i64,
        _ => return None,
    })
}

fn unique(names: &[String]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(names.len());
    for n in names {
        let mut c = n.clone();
        let mut k = 1;
        while !seen.insert(c.clone()) {
            c = format!("{n}_{k}");
            k += 1;
        }
        out.push(c);
    }
    out
}

fn write_lang(
    stem: &std::path::Path,
    header: &str,
    module: &str,
    open_module: &str,
    close_module: &str,
    open_type: &str,
    close_type: &str,
    field_tpl: &str,
    member_tpl: &str,
    classes: &[TypeOut],
    enums: &[(String, Vec<(String, i64)>)],
) {
    let ids = unique(
        &classes
            .iter()
            .map(|c| format!("t_{}", ident(&c.name)))
            .chain(enums.iter().map(|e| format!("t_{}", ident(&e.0))))
            .collect::<Vec<_>>(),
    );
    let mut s = String::from(header);
    s.push_str(&open_module.replace("{m}", &ident(module)));
    for (i, c) in classes.iter().enumerate() {
        s.push_str(&open_type.replace("{n}", &ids[i]));
        let fids = unique(
            &c.fields
                .iter()
                .map(|(f, _)| format!("f_{}", ident(f)))
                .collect::<Vec<_>>(),
        );
        for (j, (_, o)) in c.fields.iter().enumerate() {
            s.push_str(&field_tpl.replace("{f}", &fids[j]).replace("{v}", &format!("0x{o:X}")));
        }
        s.push_str(close_type);
    }
    for (i, (_, members)) in enums.iter().enumerate() {
        s.push_str(&open_type.replace("{n}", &ids[classes.len() + i]));
        let mids = unique(
            &members
                .iter()
                .map(|(m, _)| format!("f_{}", ident(m)))
                .collect::<Vec<_>>(),
        );
        for (j, (_, v)) in members.iter().enumerate() {
            s.push_str(&member_tpl.replace("{f}", &mids[j]).replace("{v}", &v.to_string()));
        }
        s.push_str(close_type);
    }
    s.push_str(close_module);
    std::fs::write(stem, s).expect("write output file");
}

fn write_json(
    stem: &std::path::Path,
    file: &str,
    classes: &[TypeOut],
    enums: &[(String, Vec<(String, i64)>)],
) {
    let mut s = format!("{{\n  \"{}\": {{\n    \"classes\": {{\n", esc(file));
    for (i, c) in classes.iter().enumerate() {
        s.push_str(&format!("      \"{}\": {{\n        \"fields\": {{\n", esc(&c.name)));
        for (j, (f, o)) in c.fields.iter().enumerate() {
            let comma = if j + 1 == c.fields.len() { "" } else { "," };
            s.push_str(&format!("          \"{}\": {o}{comma}\n", esc(f)));
        }
        let comma = if i + 1 == classes.len() { "" } else { "," };
        s.push_str(&format!(
            "        }},\n        \"metadata\": [],\n        \"parent\": \"{}\"\n      }}{comma}\n",
            esc(&c.parent)
        ));
    }
    s.push_str("    },\n    \"enums\": {\n");
    for (i, (e, members)) in enums.iter().enumerate() {
        s.push_str(&format!("      \"{}\": {{\n", esc(e)));
        for (j, (m, v)) in members.iter().enumerate() {
            let comma = if j + 1 == members.len() { "" } else { "," };
            s.push_str(&format!("        \"{}\": {v}{comma}\n", esc(m)));
        }
        let comma = if i + 1 == enums.len() { "" } else { "," };
        s.push_str(&format!("      }}{comma}\n"));
    }
    s.push_str("    }\n  }\n}\n");
    std::fs::write(stem, s).expect("write output file");
}

fn dump_all(w: &World) {
    let dir = std::path::Path::new("output");
    std::fs::create_dir_all(dir).expect("create output directory");

    let mut version = String::from("unknown");
    let mut written: Vec<String> = Vec::new();

    for (ai, asm) in w.asms.iter().enumerate() {
        let file = asm.file.clone();
        if !file.starts_with("osu.") {
            continue;
        }
        if version == "unknown" && file == "osu.Game.dll" {
            if let Some(a) = &asm.res.assembly {
                version = format!(
                    "{}.{}.{}",
                    a.version.major, a.version.minor, a.version.build
                );
            }
        }

        let mut classes: Vec<TypeOut> = Vec::new();
        let mut enums: Vec<(String, Vec<(String, i64)>)> = Vec::new();
        for (idx, td) in asm.res.enumerate_type_definitions() {
            if !matches!(td.flags.kind, Kind::Class) {
                continue;
            }
            let t = Ty { ai, idx };
            let name = w.name_of(t);
            let base = w.extends_of(t).map(|p| w.name_of(p));
            match base.as_deref() {
                Some("System.Enum") => {
                    let members: Vec<(String, i64)> = td
                        .fields
                        .iter()
                        .filter(|f| f.literal)
                        .filter_map(|f| Some((f.name.to_string(), constant_value(f.default.as_ref()?)?)))
                        .collect();
                    if !members.is_empty() {
                        enums.push((name, members));
                    }
                }
                Some("System.ValueType") => {}
                _ => {
                    let fields = w.layout(t, Algo::CoreClr, 0, true).0;
                    if fields.is_empty() {
                        continue;
                    }
                    classes.push(TypeOut {
                        name,
                        parent: base.unwrap_or_default(),
                        fields,
                    });
                }
            }
        }
        classes.sort_by(|a, b| a.name.cmp(&b.name));
        enums.sort_by(|a, b| a.0.cmp(&b.0));

        let module = ident(&file);
        let stem = dir.join(&module);
        let rs = stem.with_extension("rs");
        let hpp = stem.with_extension("hpp");
        let cs = stem.with_extension("cs");
        let zig = stem.with_extension("zig");

        write_json(&stem.with_extension("json"), &file, &classes, &enums);
        write_lang(
            &rs,
            "#![allow(non_upper_case_globals, non_snake_case, non_camel_case_types, dead_code)]\n\n",
            &module,
            "pub mod {m} {\n",
            "}\n",
            "    pub mod {n} {\n",
            "    }\n",
            "        pub const {f}: usize = {v};\n",
            "        pub const {f}: i64 = {v};\n",
            &classes,
            &enums,
        );
        write_lang(
            &hpp,
            "#pragma once\n\n#include <cstddef>\n#include <cstdint>\n\n",
            &module,
            "namespace {m} {\n",
            "}\n",
            "    namespace {n} {\n",
            "    }\n",
            "        constexpr std::ptrdiff_t {f} = {v};\n",
            "        constexpr std::int64_t {f} = {v};\n",
            &classes,
            &enums,
        );
        write_lang(
            &cs,
            "using System;\n\n",
            &module,
            "public static class {m}\n{\n",
            "}\n",
            "    public static class {n}\n    {\n",
            "    }\n",
            "        public const nint {f} = {v};\n",
            "        public const long {f} = {v};\n",
            &classes,
            &enums,
        );
        write_lang(
            &zig,
            "",
            &module,
            "pub const {m} = struct {\n",
            "};\n",
            "    pub const {n} = struct {\n",
            "    };\n",
            "        pub const {f}: usize = {v};\n",
            "        pub const {f}: i64 = {v};\n",
            &classes,
            &enums,
        );

        println!("{file}: {} classes, {} enums", classes.len(), enums.len());
        written.push(file);
    }

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let list = written
        .iter()
        .map(|n| format!("    \"{n}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    let info = format!(
        "{{\n  \"game_version\": \"{version}\",\n  \"build_number\": \"{version}\",\n  \"source\": \"osunayo\",\n  \"timestamp\": {timestamp},\n  \"assemblies\": [\n{list}\n  ]\n}}\n"
    );
    std::fs::write(dir.join("info.json"), info).expect("write info.json");
    println!("\nwrote {} modules to {}", written.len(), dir.display());
}

fn main() {
    let base = env::var("OSU_LAZER")
        .or_else(|_| env::var("LOCALAPPDATA").map(|p| format!("{p}\\osulazer\\current")))
        .unwrap_or_else(|_| ".".into());
    let names = [
        "osu.Framework.dll",
        "osu.Game.dll",
        "osu.Game.Rulesets.Osu.dll",
        "osu.Game.Rulesets.Mania.dll",
        "osu.Game.Rulesets.Taiko.dll",
        "osu.Game.Rulesets.Catch.dll",
        "osuTK.dll",
        "System.Private.CoreLib.dll",
        "Realm.dll",
        "Newtonsoft.Json.dll",
        "Sentry.dll",
        "osu.Game.Auth.dll",
        "osu!.dll",
    ];
    let w = World::load(&names.map(|n| format!("{base}\\{n}")));
    println!("assemblies loaded: {}", w.asms.len());
    println!("types indexed: {}\n", w.index.len());

    let args: Vec<String> = env::args().collect();
    let pack = !args.iter().any(|a| a == "nopack");
    match args.get(1).map(|s| s.as_str()) {
        Some("dump") if args.len() >= 3 => dump(&w, &args[2], pack),
        Some("verify") if args.len() >= 3 => verify(&w, &args[2], pack),
        _ => dump_all(&w),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fld(name: &str, size: usize, align: usize, gc: bool, isref: bool, byvalue: bool) -> Fld {
        Fld {
            name: name.to_string(),
            size,
            align,
            gc,
            isref,
            byvalue,
        }
    }

    fn at(fields: &[(String, usize)], name: &str) -> usize {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| *o)
            .unwrap()
    }

    #[test]
    fn multiplayer_room_user_matches_reference_layout() {
        let fields = vec![
            fld("UserID", 4, 4, false, false, false),
            fld("<State>", 4, 4, false, false, false),
            fld("<BeatmapAvailability>", 8, 8, true, true, false),
            fld("<Mods>", 8, 8, true, true, false),
            fld("<MatchState>", 8, 8, true, true, false),
            fld("RulesetId", 8, 4, false, false, true),
            fld("BeatmapId", 8, 4, false, false, true),
            fld("VotedToSkipIntro", 1, 1, false, false, false),
            fld("Role", 4, 4, false, false, false),
            fld("<User>", 8, 8, true, true, false),
        ];
        let (out, size) = place_coreclr(&fields, PTR);
        assert_eq!(at(&out, "<BeatmapAvailability>"), 0x08);
        assert_eq!(at(&out, "<Mods>"), 0x10);
        assert_eq!(at(&out, "<MatchState>"), 0x18);
        assert_eq!(at(&out, "<User>"), 0x20);
        assert_eq!(at(&out, "UserID"), 0x28);
        assert_eq!(at(&out, "<State>"), 0x2C);
        assert_eq!(at(&out, "Role"), 0x30);
        assert_eq!(at(&out, "VotedToSkipIntro"), 0x34);
        assert_eq!(at(&out, "RulesetId"), 0x38);
        assert_eq!(at(&out, "BeatmapId"), 0x40);
        assert_eq!(size, 0x48);
    }

    #[test]
    fn ident_is_valid_in_every_target_language() {
        assert_eq!(ident("osu.Game.dll"), "osu_Game_dll");
        assert_eq!(ident("<Host>k__BackingField"), "_Host_k__BackingField");
        assert_eq!(ident("\u{2}"), "u0002");
        assert_eq!(ident("<"), "t_");
        assert_eq!(ident("2fast"), "_2fast");
    }

    #[test]
    fn esc_escapes_control_characters() {
        assert_eq!(esc("a\"b\\c\u{2}"), "a\\\"b\\\\c\\u0002");
    }

    #[test]
    fn unique_disambiguates_collisions() {
        let names: Vec<String> = ["a", "a", "a", "a_1"].iter().map(|s| s.to_string()).collect();
        assert_eq!(unique(&names), vec!["a", "a_1", "a_2", "a_1_1"]);
    }
}
