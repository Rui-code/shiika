# Classes

## Class definition

Example

```sk
class A
  # A class method
  def self.foo -> Int
    1
  end

  # An instance method
  def bar -> Int
    2
  end
end

p A.foo #=> 1
p A.new.bar #=> 2
```

## Instance variables

Name of an instance variable starts with `@`. All instance variables of a class must be initialized in the method `initialize`.

Example

```sk
class Book
  def initialize(title: String, price: Int)
    @title = title
    @price = price
  end
end
```

For convenence, this can be written as:

```sk
class Book
  def initialize(@title: String, @price: Int); end
end
```

Instance variables are readonly by default. To make it reassignable, declare it with `var`.

## Accessors

For each instance variable, accessor methods are automatically defined. A reader method for an readonly one, reader and setter method for an writable one.

Example

```sk
class Person
  def initialize(name: String, age: Int)
    @name = name
    var @age = age
  end
end

taro = Person.new("Taro", 20)
p taro.name #=> "Taro"
p taro.age  #=> 20
taro.age += 1

taro.name = "Jiro" # This is error because @name is not declared with `var`.
```

## Visibility

Shiika does not have visibility specifier like `private` or `protected`. Conventionally, it is preferred to prefix `_` for instance variables which are intended "internal".

```sk
class Person
  def initialize(name: String, age: Int)
    @name = name
    var @age = age
    @_secret_count = 0
  end
end
```

In this case `Person.new._secret_count` is valid but normally you should avoid this because it is considered "private". Private method of a library is rather an implementation detail than public API. It may be silently changed in the future version of the library.

Shiika allows this for in case you _really_ need it.

