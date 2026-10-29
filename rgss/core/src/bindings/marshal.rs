//! Ruby Marshal 4.8 serialization for RGSS event graphs and data.
use super::*;

pub(super) fn install(vm: &mut Vm) {
    install_module(
        vm,
        "Marshal",
        &[("dump", dump), ("load", load), ("restore", load)],
    );
}

fn dump(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if args.is_empty() || args.len() > 3 {
        return Err(vm.raise_arg("Marshal.dump expects an object and optional IO/depth"));
    }
    let (io, limit) = match args.get(1).copied() {
        Some(Value::Int(limit)) => (Value::Nil, limit),
        io => (
            io.unwrap_or(Value::Nil),
            args.get(2).copied().map_or(-1, |v| value_i64(v, -1)),
        ),
    };
    let mut writer = Encoder {
        vm,
        bytes: vec![4, 8],
        objects: HashMap::new(),
        symbols: HashMap::new(),
        object_count: 0,
    };
    writer.value(args[0], limit)?;
    let bytes = writer.bytes;
    let result = vm.str_new(&bytes);
    vm.str_set_binary(result, true);
    if io != Value::Nil {
        let write = vm.intern("write");
        vm.funcall(io, write, &[result], Value::Nil)?;
        Ok(io)
    } else {
        Ok(result)
    }
}

fn load(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if args.len() != 1 {
        return Err(vm.raise_arg("Marshal.load expects a String or IO"));
    }
    let input = args[0];
    let is_string = vm.str_bytes(input).is_some();
    let data = if is_string {
        input
    } else {
        let read = vm.intern("read");
        vm.funcall(input, read, &[], Value::Nil)?
    };
    let bytes = vm.expect_str(data, "Marshal data")?;
    let mut decoder = MarshalDecoder::new(vm, &bytes);
    let value = decoder.decode_stream()?;
    let consumed = decoder.offset;
    if !is_string && consumed < bytes.len() {
        let pos = vm.intern("pos");
        let set_pos = vm.intern("pos=");
        if vm.respond_to(input, pos) && vm.respond_to(input, set_pos) {
            let current = vm.funcall(input, pos, &[], Value::Nil)?;
            let position = value_i64(current, 0) - (bytes.len() - consumed) as i64;
            vm.funcall(input, set_pos, &[Value::Int(position)], Value::Nil)?;
        }
    }
    Ok(value)
}

struct Encoder<'a> {
    vm: &'a mut Vm,
    bytes: Vec<u8>,
    objects: HashMap<ObjId, usize>,
    symbols: HashMap<sabiruby::symbol::Sym, usize>,
    object_count: usize,
}

impl Encoder<'_> {
    fn long(&mut self, n: i64) {
        if n == 0 {
            self.bytes.push(0);
        } else if (1..123).contains(&n) {
            self.bytes.push((n + 5) as u8);
        } else if (-123..0).contains(&n) {
            self.bytes.push((n - 5) as u8);
        } else {
            let bytes = n.to_le_bytes();
            let mut len = 8;
            while len > 1 && bytes[len - 1] == if n < 0 { 255 } else { 0 } {
                len -= 1;
            }
            self.bytes
                .push(if n < 0 { -(len as i8) as u8 } else { len as u8 });
            self.bytes.extend_from_slice(&bytes[..len]);
        }
    }

    fn data(&mut self, bytes: &[u8]) {
        self.long(bytes.len() as i64);
        self.bytes.extend_from_slice(bytes);
    }

    fn symbol(&mut self, name: &str) {
        let symbol = self.vm.intern(name);
        if let Some(index) = self.symbols.get(&symbol).copied() {
            self.bytes.push(b';');
            self.long(index as i64);
        } else {
            self.symbols.insert(symbol, self.symbols.len());
            self.bytes.push(b':');
            self.data(name.as_bytes());
        }
    }

    fn value(&mut self, value: Value, limit: i64) -> Result<(), VmError> {
        if limit == 0 {
            return Err(self.vm.raise_arg("exceed depth limit"));
        }
        let next = if limit < 0 { -1 } else { limit - 1 };
        match value {
            Value::Nil => self.bytes.push(b'0'),
            Value::True => self.bytes.push(b'T'),
            Value::False => self.bytes.push(b'F'),
            Value::Int(n) if (-0x4000_0000..0x4000_0000).contains(&n) => {
                self.bytes.push(b'i');
                self.long(n);
            }
            Value::Int(n) => {
                self.object_count += 1;
                self.bigint(n < 0, n.unsigned_abs().to_le_bytes().to_vec());
            }
            Value::Float(n) => {
                self.object_count += 1;
                self.bytes.push(b'f');
                let text = if n.is_nan() {
                    "nan".to_owned()
                } else {
                    n.to_string()
                };
                self.data(text.as_bytes());
            }
            Value::Sym(symbol) => {
                let name = self.vm.sym_name(symbol);
                self.symbol(&name);
            }
            Value::Obj(object) => {
                if let Some(index) = self.objects.get(&object).copied() {
                    self.bytes.push(b'@');
                    self.long(index as i64);
                    return Ok(());
                }
                self.objects.insert(object, self.object_count);
                self.object_count += 1;
                if let Some(bytes) = self.vm.str_bytes(value).map(<[u8]>::to_vec) {
                    let binary = self.vm.str_binary(value);
                    if !binary {
                        self.bytes.push(b'I');
                    }
                    self.bytes.push(b'"');
                    self.data(&bytes);
                    if !binary {
                        self.long(1);
                        self.symbol("E");
                        self.bytes.push(b'T');
                    }
                } else if let Some(values) = self.vm.ary_vals(value) {
                    self.bytes.push(b'[');
                    self.long(values.len() as i64);
                    for value in values {
                        self.value(value, next)?;
                    }
                } else if let Some(entries) = self.vm.hash_entries(value) {
                    let default = match &self.vm.heap.get(object).kind {
                        ObjKind::Hash(hash) => hash.default.get(),
                        _ => Value::Nil,
                    };
                    self.bytes
                        .push(if default == Value::Nil { b'{' } else { b'}' });
                    self.long(entries.len() as i64);
                    for (key, value) in entries {
                        self.value(key, next)?;
                        self.value(value, next)?;
                    }
                    if default != Value::Nil {
                        self.value(default, next)?;
                    }
                } else if let ObjKind::BigInt(n) = &self.vm.heap.get(object).kind {
                    let negative = n.sign() < 0;
                    let mut bytes = n.hash_bytes();
                    bytes.pop();
                    self.bigint(negative, bytes);
                } else if matches!(self.vm.heap.get(object).kind, ObjKind::Object) {
                    self.bytes.push(b'o');
                    let name = self.vm.class_name(self.vm.real_class_of(value));
                    self.symbol(&name);
                    let ivars: Vec<_> = self
                        .vm
                        .heap
                        .get(object)
                        .ivars
                        .iter()
                        .map(|(name, value)| (self.vm.sym_name(*name), value.get()))
                        .collect();
                    self.long(ivars.len() as i64);
                    for (name, value) in ivars {
                        self.symbol(&name);
                        self.value(value, next)?;
                    }
                } else {
                    return Err(self.vm.raise_type("unsupported object in Marshal.dump"));
                }
            }
        }
        Ok(())
    }

    fn bigint(&mut self, negative: bool, mut bytes: Vec<u8>) {
        while bytes.last() == Some(&0) {
            bytes.pop();
        }
        if bytes.len() % 2 != 0 {
            bytes.push(0);
        }
        self.bytes.push(b'l');
        self.bytes.push(if negative { b'-' } else { b'+' });
        self.long((bytes.len() / 2) as i64);
        self.bytes.extend_from_slice(&bytes);
    }
}
