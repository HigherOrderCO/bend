type Type = (A) | (B)

def myFun(a):
  match a:
    case Type/A:
      return 1
    case Type/B:
      return 2