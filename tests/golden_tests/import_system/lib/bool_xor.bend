use types{Bool}

(bool_xor Bool/True Bool/False) = Bool/True
(bool_xor Bool/False Bool/True) = Bool/True
(bool_xor * *) = Bool/False
