# Syntax

This file provides a reference of each possible syntax of bend programming language.

Click [here](#imp-syntax) to see the syntax for "imp", the variant of bend that looks like an imperative language like python.

Click [here](#fun-syntax) to see the syntax for "fun", the variant of bend that looks like a functional language like Haskell or ML.

Both syntaxes can be mixed in the same file like the example below:

```python
object Point { x, y }

type MyTree = (Node ~left ~right) | (Leaf value)

type Bool:
  True
  False

def identity(x):
  return x

main =
  let result = (identity 41)
  (+ result 1)
```

<div id="imp-syntax"></div>

# Imp Syntax

## Top-level definitions

### Def

Defines a top level function.

```python
def add(x, y):
  result = x + y
  return result

def main:
  return add(40, 2)
```

A function definition is composed by a name, a sequence of parameters and a body.

A top-level name can be anything matching the regex `[A-Za-z0-9_.-/]+`, except it can't have `__` (used for generated names) or start with `//`.

The last statement of each function must either be a `return` or a selection statement (`if`, `switch`, `match`, `fold`)
where all branches `return`.

### Type

Defines an algebraic data type.

```python
type Option:
  Some { value }
  None

type Tree:
  Node { value, ~left, ~right }
  Leaf
```

Type names must be unique, and should have at least one constructor.

Each constructor is defined by a name followed by its fields.

The `~` notation indicates a recursive field. To use `fold` statements with a type its recursive fields must be correctly marked with `~`.

The constructor names inherit the name of their types and become functions (`Tree/Node` and `Tree/Leaf` in this case).
The exact function they become depends on the encoding.

Read [defining data types](./defining-data-types.md) to know more.

### Object

Defines a type with a single constructor (like a struct, a record or a class).

```python
object Pair { fst, snd }

object Function { name, args, body }

object Vec { len, data }
```

The constructor created from this definition has the same name as the type.

Since it only has one constructor, `fold`ing a recursive `object` requires some additional stop condition apart from pattern matching on the value itself (like an `if` statement).

## Statements

### Assignment

```python
value = 2
return value

(first, second) = (1, 2)
return second

{x y} = {2 3}
```

Assigns a value to a variable.

It's possible to assign to a pattern, like a tuple or superposition, which will destructure the value returned by the expression.

```python
(first, second) = (1, 2)
```

### Use

```rust
use x = 2 + 3
return x + x
```

Inline copies of the declared bind, it is equivalent to this code:

```rust
return ((2 + 3) + (2 + 3))
```

### In-Place Operation

```python
x += 1
return x
```

The in-place operation does an infix operation and re-assigns a variable.

The operations are:

- Addition `+=`
- Subtraction `-=`
- Multiplication `*=`
- Division `/=`
- Bit And `&=`
- Bit Or `|=`
- Bit Xor `^=`
- Mapper `@=`

The mapper in-place operation applies a function and re-assigns the variable:

```python
x = "hello"
x @= String/uppercase
```

### Return

```python
return "hello"
```

Returns the expression that follows. The last statement of each branch of a function must be a `return`.

```py
# Allowed, all branches return
def max(a, b):
  if a > b:
    return a
  else:
    return b
```

```py
# Not allowed, early return
def Foo(x):
  if test_condition(x):
    return "err"
  else:
    y = map(x)

  return y
```

```py
# Not allowed, one of the branches doesn't return
def Foo(a, b):
  if a < b:
    return a
  else:
    c = a + b
```

### If

```python
if condition:
  return 0
else:
  return 1
```

A branching statement where `else` is mandatory.

The condition must return a `u24` number, where 0 will run the `else` branch and any other value will return the first one.

It is possible to make if-chains using `elif`:

```python
if condition1:
  return 0
elif condition2:
  return 1
elif condition3:
  return 2
else:
  return 3
```

The conditions are evaluated in order, one by one, stopping at the first successful case.

### Switch

```python
switch x = 5:
  case 0:
    return 6
  case 1:
    return 7
  case _:
    return x-2
```

A switch binds a variable name to the result of a given condition and branches to the case matching its value. Cases
must be listed from least to greatest, beginning with `0` and incrementing by 1. The last case must be `_`, which
catches all values not explicitly enumerated. Switches may only be used with native numbers values.

In the last case, the predecessor value is available with the name `bound_var-next_num`, where `bound_var` is the variable
set by the condition and `next_num` is the expected value of the next case. For example, the above example code returns
`3`, since `x-2` is bound to `5 - 2` and the value of `x` doesn't match any explicit case.

This switch statement is equivalent to the `if` from the previous section:

```python
switch _ = condition:
  case 0:
    # else branch
    return 1
  case _:
    # then branch
    return 0
```

### Match

```python
match x = Option/none:
  case Option/some:
    y = x.value
  case Option/none:
    y = 0
```

A pattern matching statement, the cases must be the constructor names of the matching value.

It is possible to bind a variable name to the matching value. The fields of the matched constructor are bound to `matched_var.field_name`.

### Fold

```python
fold x = Tree/leaf:
  case Tree/node:
    return x.value + x.left + x.right
  case Tree/leaf:
    return 0
```

A fold statement. Reduces the given value with the given match cases.

It is possible to bind a variable name to the matching value. Just like in `match`, the fields are bound to `matched_var.field_name`.

For fields notated with `~` in the type definition, the fold function is called implicitly.

It is equivalent to the inline recursive function:

```python
def fold(x):
  match x:
    case Tree/Node:
      return x.value + fold(x.left) + fold(x.right)
    case Tree/Leaf:
      return 0
...
fold(Tree/Leaf)
```

### Bend

Bend can be used to create recursive data structures:

```rust
bend x = 0:
  when x < 10:
    left = fork(x + 1)
    right = fork(x + 1)
    y = Tree/Node(left, right)
  else:
    y = Tree/Leaf(x)
```

Which binds a variable to the return of an inline recursive function.
The function `fork` is available inside the `when` arm of the `bend` and calls it recursively.

It is possible to pass multiple state variables, which can be initialized:

```python
bend x = 1, y = 2 ...:
  when condition(x, y, ...):
    ...
```

When calling `fork`, the function must receive the same number of arguments as the number of state variables.

It is equivalent to this inline recursive function:

```python
def bend(x, y, ...):
  if condition(x, y, ...):
    ...
    return ... bend(x, y, ...) ...
  else:
    return ...
```

### Open

```python
p = Point { x: 1, y: 2 }
open Point: p
return Point { x: p.x * p.x, y: p.y * p.y }
```

Brings the inner fields of an object into scope. The original variable can still be accessed, but doing so will cause any used fields to be duplicated.

It's equivalent to pattern matching on the object, with the restriction that its type must have only one constructor.

```python
open Point: p
...

# Equivalent to:
match p:
  Point:
    ...
```

### With block

```python
with Result:
  x <- safe_div(2, 0)
  return x
```

A monadic `with` block.

Where `x <- ...` performs a monadic operation.

Expects `Result` to be a type defined with `type` or `object` and the function `Result/bind` to be defined.
The monadic bind function should be of type `(Result a) -> (a -> Result b) -> Result b`, like this:

```python
def Result/bind(res, nxt):
  match res:
    case Result/Ok:
      nxt = undefer(nxt)
      return nxt(res.value)
    case Result/Err:
      return res
```

However, the second argument, `nxt`, is actually a deferred call to the continuation, passing any free variables as arguments.
Therefore, all `bind` functions must call the builtin function `undefer` before using the value of `nxt`, as in the example above.
This is necessary to ensure that the continuation in recursive monadic functions stays lazy and doesn't expand infinitely.

This is an example of a recursive function that would loop if passing the variable `a` to the recursive call `Result/foo(a, b)` was not deferred:

```python
def Result/foo(x, y):
  with Result:
    a <- Result/Ok(1)
    if b:
      b = Result/Err(x)
    else:
      b = Result/Ok(y)
    b <- b
    return Result/foo(a, b)
```

Other statements are allowed inside the `with` block and it can both return a value at the end and bind a variable, like branching statements do.

```python
# Also ok:
with Result:
  x <- safe_div(2, 0);
  y = x
return y
```

The name `wrap` is bound inside a `with` block as a shorthand for `Type/wrap`,
and it calls the unit function of the monad, also called `pure` in some languages:

```python
def Result/wrap(x):
  return Result/Ok(x)

with Result:
  x <- some_operation(...)
  y <- some_operation(...)
  return wrap(x * y)
```

### Def

Creates a local function visible in the current block capturing variables:

```python
def main:
  y = 41
  x = 1
  def aux_add(x):
    return x + y
  return aux_add(x)
```

## Expressions

### Variables

```python
some_var

foo/bar
```

A variable can be anything matching the regex `[A-Za-z0-9_.-/]+` but with some restrictions:

- It can not start with `//`
- It can not contain `__`

A variable is a name for some immutable expression. It is possible to rebind variables with the same name.

```python
x = 1
x = x + 1
```

Note that `-` is also used for negative numbers and as the numeric operator. Bend's grammar is greedily parsed from left to right, meaning that `x-3` always represents a name and not `x - 3` or a sequence of expressions like in `[x -3]`.

### Lambdas

```python
lambda x: x

lambda x, y: y

λx y: x
```

Lambdas represents anonymous inline functions, it can bind a variable and has an expression as body.

Using `,` is optional.

### Unscoped Lambdas and Variables

```python
lambda $x: $x

λ$x $y: $x
```

Like lambdas, with the exception that the variable starts with a `$` sign. Every unscoped variable in a function must have a unique name and must be used exactly once.

Unscoped variables are not transformed and linearized like normal scoped variables.

Read [using scopeless lambdas](/docs/using-scopeless-lambdas.md) to know more about their behavior.

### Function Call

```python
callee(arg_1, arg_2, arg_n)
```

A call is written with a callee followed by a list of arguments. Arguments can be optionally separated by `,`.

The effect of a function call is to substitute the callee with it's body and replace the arguments by the passed variables.

The called function can be any expression and it supports partial applications.

Optionally, if you call a function by its name, you can used named arguments:

```python
callee(expr1, expr2, arg4 = expr3, arg3 = expr4)
```

In case named arguments are used, they must come after the positional arguments and the function must be called with exactly the number of arguments of its definition.

### Eraser

```python
*

eraser = *

*(41 + 1)  # applies 41 + 1 to `*` erasing the number and returns `*`

* = 41 + 1 # erases 41 + 1
```

The effect of an eraser is to free memory. Erasers behave like a `null`.

It's impossible to compare or match eraser values.

It is implicitly inserted for variables that have not been used:

```python
def constant(x):
  return 8345
```

### Tuple

```python
(3, 9)
```

A Tuple is surrounded by `(` `)` and should contain 2 or more elements. Elements are separated by `,`.

### Superposition

```python
{1 2 3}
```

A superposition of values is defined using `{` `}` with at least 2 expressions inside. Elements can be optionally separated by `,`.

Read [sups and dups](./dups-and-sups.md) to know more.

### Numbers and Infix Operations

Currently, bend supports 3 types of numbers: floats, integers and unsigned integers. All of then are 24 bit sized.

```python
f24 = +88.012

i24 = -42

u24 = 42
```

Currently, the 3 number types cannot be mixed.

| Operation             | Syntax   | Supported Types  |
| --------------------- | -------- | ---------------- |
| Addition              | x + y    | int, float, uint |
| Subtraction           | x - y    | int, float, uint |
| Multiplication        | x \* y   | int, float, uint |
| Division              | x / y    | int, float, uint |
| Remainder             | x % y    | int, float, uint |
| Exponentiation        | x \*\* y | float            |
| Equal                 | x == y   | int, float, uint |
| Not Equal             | x != y   | int, float, uint |
| Less Than             | x < y    | int, float, uint |
| Greater Than          | x > y    | int, float, uint |
| Less Than or Equal    | x <= y   | int, float, uint |
| Greater Than or Equal | x >= y   | int, float, uint |
| Bitwise And           | x & y    | int, uint        |
| Bitwise Or            | x \| y   | int, uint        |
| Bitwise Xor           | x ^ y    | int, uint        |

### Constructor Literals

Constructors are just functions.
A Constructor expression is equivalent to calling a Constructor function, they have 2 syntaxes:

```python
# Constructor syntax, requires all field names
Type/Ctr { field1: 4, field2: 8 }

# Function syntax
Type/Ctr(field1 = 4, field2 = 8)

Type/Ctr(4, field2 = 8)

Type/Ctr(4, 8)

Type/Ctr(4) # Can be partially applied if not using named arguments
```

### Character Literal

```python
'x'
```

A Character is surrounded with `'`. Accepts unicode characters, unicode escapes in the form '\u{hex value}' and is desugared to the unicode codepoint as an `u24`.

Only supports unicode codepoints up to `0xFFFFFF`.

### Symbol Literal

```python
# Becomes 2146 (33 << 6 + 34)
`hi`
```

A Symbol encodes a up to 4 base64 characters as a `u24` number. It is surrounded by `\``.

Empty characters are interpreted as `A` which has value 0, meaning that `B` is the same as `AAAB`.

### String Literal

```python
"Hello, World!"
```

A String literal is surrounded with `"`. Accepts the same values as characters literals.

It is desugared to constructor calls of the built-in type String, `String/cons(head, ~tail)` and `String/nil` .

### List Literal

```python
[1, 2, "three"]
```

A List literal is surrounded by `[` `]`. The elements must be separated by `,`.

It is desugared to constructor calls of the built-in type List, `List/cons(head, ~tail)` and `List/nil` .

### Tree Literals

```python
![![1, 2], ![3, 4]]
```

The Tree literals `![]` and `!` are used to create values of the built-in type `Tree`.

`![a b]` is equivalent to `Tree/Node(a, b)`.

`!x` is equivalent to `Tree/Leaf(x)`.

### Map Literals

```python
{ 0: 4, `hi`: "bye", 'c': 2 + 3 }
x[0] = 5     # Assigns the key 0 to the value 5
return x[0]  # Gets the value of the key 0
```

Bend has a built-in binary tree map data structure where the key is a `u24`, meaning you can use numbers, characters, and symbols as keys.

### List Comprehension

```python
[x + 1 for x in list]

[x + 1 for x in list if x > 2]
```

A List Comprehension generates a new list, it can be extracted in 3 parts.

`[expression . iterator . condition]`

Expression: The expression to be performed in the iterator element.

Iterator: Binds a name to the list elements.

Condition: Optional, is used to filter the list elements.

It is desugared to a fold statement:

```python
fold list:
  List/cons:
    if condition:
      List/cons(list.head, list.tail)
    else:
      list.tail
  List/nil:
    List/nil
```

<div id="fun-syntax"></div>

# Fun Syntax

## Top-level definitions

```rust
type Name
  = (Ctr1 arg1 arg2)
  | Ctr2

Name (Ctr1 sub_arg1 sub_arg2) arg3 = rule0_body
Name Ctr2 arg3 = rule1_body
```

A top-level name can be anything matching the regex `[A-Za-z0-9_.-/]+`, except it can't have `__` (used for generated names) or start with `//`.

### Function Definitions

A function definition is composed of a sequence of pattern matching equations.
Each rule is the name of the function, a sequence of patterns and then the body.

```rust
identity x = x

(Bool.neg True)  = False
(Bool.neg False) = True

MapMaybe (Some val) f = (Some (f val))
MapMaybe None f = None

Pair.get (fst, snd) f = (f fst snd)
```

A rule pattern can be:

- A variable.
- A number.
- A constructor.
- A tuple.
- A superposition.
- A wildcard `*`.

And the builtin types that desugar to one of the above:

- A list (becomes a constructor).
- A string (becomes a constructor).
- A natural number (becomes a constructor).
- A character (becomes a number).
- A symbol (becomes a number);

Unscoped variables can't be defined in a rule pattern.

The rule body is a term, there are no statements in the Fun variant of Bend.

Read [pattern matching](./pattern-matching.md) to learn about what exactly the rules for pattern matching equations are.

### Type

Defines an Algebraic Data Type, it should have at least one constructor.

```rust
type Tree
  = (Leaf value)
  | (Node ~left ~right)
  | Nil
```

`Tree` is the ADT name and it should be unique, except that it can be used once by a constructor name.

Each constructor is defined by a name followed by its fields. The `~` notation describes a recursive field.

The constructors inherit the name of their types and become functions (`Tree/Node` and `Tree/Leaf` in this case).

## Terms

### Variables

A variable can be anything matching the regex `[A-Za-z0-9_.-/]+` but with some restrictions:

- It can not start with `//`
- It can not contain `__`

A variable is a name for some immutable expression. It is possible to rebind variables with the same name.

```rust
let x = 1
let x = (+ x 1)
```

### Lambda

```rust
@x x

λx x

λ(fst, snd) snd

λ{x y} x
```

Lambdas represents anonymous inline functions, it can be written with `λ` or `@` followed by a pattern and a term.

A tuple or duplication pattern is equivalent to a lambda followed by a `let`.

```rust
λ(fst, snd) snd
λa let (fst, snd) = a; snd

λ{x y} (x y)
λa let {x y} = a; (x y)
```

### Unscoped Variables

```rust
λ$x $x
```

Like a normal scoped variable, but starts with a `$` sign. Every unscoped variable in a function must have a unique name and must be used exactly once.
They can be defined anywhere a scoped variable would be defined in a term, like in a lambda or a `let`.

Unscoped variables are not transformed and linearized like normal scoped variables.

Read [using scopeless lambdas](/docs/using-scopeless-lambdas.md) to know more about.

### Application

```rust
(fun arg_1 arg_2 ... arg_n)
```

An application is surrounded by `(` `)`, written in lisp style.

> Lambdas have a higher precedence, so `(@x x 1)` and `((@x x) 1)` means the same thing.

### Tuples

```rust
(1, 2, 3)
```

A tuple is surrounded by `(` `)`, with the difference that it's elements are separated by `,`.

### Superposition

```rust
{1 2 3}
```

A superposition of values is defined using `{` `}` with at least 2 terms inside.

Read [sups and dups](./dups-and-sups.md) to know more.

### Let-bindings

```rust
let x = (+ 1 2)
x

let (fst, snd, era) = (1, 2, *);
(+ fst snd)

let {f1 f2} = λx x;
(f1 f2)

let $x = (some_fn $x);
*
```

> `*` is an eraser term.

A let term uses a pattern, it can be:

- A variable / unscoped variable.
- A tuple.
- A superposition.

The let term will expects a binding value followed by a `next` term.

Using `;` is optional.

### Use

```rust
use x = (+ 2 3)
(+ x x)
```

Inline copies of the declared bind, it is equivalent to this code:

```rust
(+ (+ 2 3) (+ 2 3))
```

### Switch

```rust
switch n {
  0: "zero"
  1: "one"
  _: "greater than 1"
}

switch x = (+ 1 1) {
  0: 42;
  _: x-1;
}
```

A switch for native numbers, it can hold a name binding if the matching term is not a variable.

The cases need to be typed from `0` to a wildcard `_` in sequence.

In the last case, the predecessor value is available with the name `bound_var-next_num`, where `bound_var` is the variable
set by the condition and `next_num` is the expected value of the next case. For example, the above example code returns
`1`, since `x-1` is bound to `(+ 1 1) - 1` and the value of `x` doesn't match any explicit case.

Using `;` is optional.

### Match

```rust
match opt = (Some "Bend") {
  Some: opt.value;
  None: "No name";
}
```

A pattern match expression, it can hold a name binding if the matching term is not a variable.

It is possible to use a _wildcard_, a named variable or `*` as default cases.

It is desugared according to the chosen encoding. Read [pattern matching](./pattern-matching.md) to know more.

Using `;` is optional.

### If

```rust
if condition {
  ...then
} else {
  ...else
}
```

A branching expression where `else` is mandatory.

The condition must return a `u24` number, where 0 will run the `else` branch and any other value will return the first one.

It is equivalent to this switch:

```rust
switch _ = condition {
  0: else
  _: then
}
```

It is possible to make if-chains using `elif`:

```rust
if condition1 {
  0
} elif condition2 {
  1
} elif condition3 {
  2
} else {
  3
}
```

### Bend

Bend can be used to create recursive data structures:

```rust
main =
  bend x = 0 {
    when (< x 3):
      (Tree/Node (fork (+ x 1)) (fork (+ x 1)))
    else:
      (Tree/Leaf x)
  }
```

Which binds a variable to the return of an inline recursive function.
The function `fork` is available inside the `when` arm of the `bend` and calls it recursively.

It is possible to pass multiple state variables, which can be initialized:

```rust
bend x = 0, y = 1 ... {
  when (condition x y ...):
    ...
}
```

When calling `fork`, the function must receive the same number of arguments as the number of state variables.

It is equivalent to this inline recursive function:

```rust
bend x y ... =
  if (condition x y ...) {
    ...
    ... (bend x y ...) ...
  } else {
    ...
  }
```

### Open

```rust
let x = (Pair 1 2);
open Pair x;
(+ x.fst x.snd)
```

Brings the inner fields of an object into scope. The original variable can still be accessed, but doing so will cause any used fields to be duplicated.

It's equivalent to pattern matching on the value, with the restriction that its type must have only one constructor.

```rust
let x = (Pair 1 2)
match x {
  Pair: (+ x.fst x.snd)
}
```

### With block

```rust
Result/bind (Result/Ok val) nxt = ((undefer nxt) val)
Result/bind err _nxt = err

div a b = switch b {
  0: (Result/Err "Div by 0")
  _: (Result/Ok (/ a b))
}

rem a b = switch b {
  0: (Result/Err "Mod by 0")
  _: (Result/Ok (% a b))
}

Main = with Result {
  ask y = (div 3 2);
  ask x = (rem y 0);
  x
}
```

Receives a type defined with `type` and expects `Result/bind` to be defined as a monadic bind function.
It should be of type `(Result a) -> (a -> Result b) -> Result b`, like in the example above.

However, the second argument, `nxt`, is actually a deferred call to the continuation, passing any free variables as arguments.
Therefore, all `bind` functions must call the builtin function `undefer` before using the value of `nxt`, as in the example above.
This is necessary to ensure that the continuation in recursive monadic functions stays lazy and doesn't expand infinitely.

This is an example of a recursive function that would loop if passing the variable `a` to the recursive call `Result/foo(a, b)` was not deferred:

```python
Result/foo x y = with Result {
  ask a = (Result/Ok 1)
  ask b = if b {
    (Result/Err x)
  } else {
    (Result/Ok y)
  }
  (Result/foo a b)
}
```

Inside a `with` block, you can use `ask`, to access the continuation value of the monadic operation.

```rust
ask y = (div 3 2)
ask x = (rem y 0)
x

# Becomes
(Result/bind (div 3 2) λy (Result/bind (rem y 0) λx x))
```

It can be used to force a sequence of operations. Since the continuation receives the result through a lambda, it is only fully evaluated after something is applied to it.

The name `wrap` is bound inside a `with` block as a shorthand for `Type/wrap`,
the equivalent as a `pure` function in other functional languages:

```rust
Result/wrap x = (Result/Ok x)

with Result {
  ask x = (some_operation ...)
  ask y = (some_operation ...)
  wrap(x * y)
}
```

### Def

Creates a local function visible in the current block capturing variables:

```rust
main =
  let base = 0
  def aux [] = base
      aux (List/Cons head tail) = (+ head (aux tail))
  (aux [1, 2, 3])
```

### Numbers and operations

Currently, bend supports 3 types of numbers: floats, integers and unsigned integers. All of then are 24 bit sized.

```rust
f24 = +88.012

i24 = -42

u24 = 42
```

Currently, the 3 number types cannot be mixed.

| Operation             | Syntax     | Supported Types  |
| --------------------- | ---------- | ---------------- |
| Addition              | (+ x y)    | int, float, uint |
| Subtraction           | (- x y)    | int, float, uint |
| Multiplication        | (\* x y)   | int, float, uint |
| Division              | (/ x y)    | int, float, uint |
| Remainder             | (% x y)    | int, float, uint |
| Exponentiation        | (\*\* x y) | float            |
| Equal                 | (== x y)   | int, float, uint |
| Not Equal             | (!= x y)   | int, float, uint |
| Less Than             | (< x y)    | int, float, uint |
| Greater Than          | (> x y)    | int, float, uint |
| Less Than or Equal    | (<= x y)   | int, float, uint |
| Greater Than or Equal | (>= x y)   | int, float, uint |
| Bitwise And           | (& x y)    | int, uint        |
| Bitwise Or            | (\| x y)   | int, uint        |
| Bitwise Xor           | (^ x y)    | int, uint        |

### Character Literal

```rust
'a'
```

A Character is surrounded with `'`. Accepts unicode characters, unicode escapes in the form '\u{hex value}' and is desugared to the unicode codepoint as an `u24`.

Only supports unicode codepoints up to `0xFFFFFF`.

### Symbol Literal

```python
# Becomes 2146 (33 << 6 + 34)
`hi`
```

A Symbol encodes a up to 4 base64 characters as a `u24` number. It is surrounded by `\``.

Empty characters are interpreted as `A` which has value 0, meaning that `B` is the same as `AAAB`.

### String Literal

```rust
"Hello"
```

A String literal is surrounded with `"`. Accepts the same values as characters literals.

The syntax above is desugared to:

```
(String.cons 'H' (String.cons 'e' (String.cons 'l' (String.cons 'l' (String.cons 'o' String.nil)))))
```

### List Literal

```rust
[1, 2, 3 4]
```

The syntax above is desugared to:

```
(List.cons 1 (List.cons 2 (List.cons 3 (List.cons 4 List.nil))))
```

Using `,` is optional.

### Tree Literals

```python
![![1, 2], ![3, 4]]
```

The Tree literals `![]` and `!` are used to create values of the built-in type `Tree`.

`![a b]` is equivalent to `Tree/Node(a, b)`.

`!x` is equivalent to `Tree/Leaf(x)`.

### Nat Literal

```rust
#3
```

The syntax above is desugared to:

```
(Nat/succ (Nat/succ (Nat/succ Nat/zero)))
```

# Native HVM definitions

```py
# This function causes two ports to be linked and returns *.
# This can be used to interpret a lambda as an application and apply something to it for example.
# It can be used like this: `let * = (link_ports @x x y)`
hvm link_ports:
  (a (b *))
  & (c a) ~ (d e)
  & (e b) ~ (d c)
```

It's also possible to define functions using HVM syntax. This can be
thought of as a way to write "HVM assembly" directly in a Bend program.
You can find the reference of this syntax in the [HVM paper](https://github.com/HigherOrderCO/HVM/blob/main/paper/PAPER.pdf).

This is meant for writing things that would otherwise be hard or
impossible to write in normal Bend syntax.

It will also ignore all term-level compiler passes and so can be
useful for writing programs with exact behaviour that won't ever be
changed or optimized by the compiler.
