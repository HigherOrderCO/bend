# Import System

## Importing Relative Paths
Paths starting with `./` or `../` are imported relative to the file.

### Example:
```py
from ./utils import helper
import ../lib/math
```

## Importing Absolute Paths
Otherwise, paths imported are relative to the folder of the main file.

### Example:
```py
from utils import helper
import lib/math
```

## Importing Specific Top-Level Names
You can import specific top-level names from a file.

### Syntax:
```py
from path/file import name
from path/file import (name1, name2)
import (path/file/name1, path/file/name2)
```

### Example:
```py
from utils/helper import calculate
from lib/math import (add, subtract)
import (lib/math/add, lib/math/subtract)
```

## Importing All Names
You can import all top-level names from a file using the wildcard `*`.

### Syntax:
```py
from path/file import *
```

## Importing All `.bend` Files from a Folder
You can import all `.bend` files from a folder using the wildcard `*`.

### Syntax:
```py
from path import *
```

## Aliasing Imports
You can alias imports to a different name for convenience.

### Importing a File with Alias
Import the `file` top-level name from `file.bend` aliased to `alias`, and all other names as `alias/name`.

### Syntax:
```py
from path import file as alias
import path/file as alias
```

### Example:
```py
from utils import helper as utilsHelper
import lib/math as mathLib
```

### Importing Specific Names with Aliases
You can import specific top-level names and alias them to different names.

### Syntax:
```py
from path/file import name as alias
from path/file import (name1 as alias1, name2 as alias2)
import (path/file/name1 as alias1, path/file/name2 as alias2)
```

### Example:
```py
from utils/helper import calculate as calc
from lib/math import (add as addFunc, subtract as subFunc)
import (lib/math/add as addFunc, lib/math/subtract as subFunc)
```
