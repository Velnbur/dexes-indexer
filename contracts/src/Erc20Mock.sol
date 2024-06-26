// SPDX-License-Identifier: MIT
pragma solidity ^0.5.16;

import {ERC20} from "openzeppelin-contracts/token/ERC20/ERC20.sol";

contract ERC20Mock is ERC20 {
    string public name;
    string public symbol;
    uint8 internal _decimals;

    constructor(
        string memory name_,
        string memory symbol_,
        uint8 decimalPlaces_
    ) public {
	name = name_;
	symbol = symbol_;
        _decimals = decimalPlaces_;
    }

    function decimals() public view returns (uint8) {
        return _decimals;
    }

    function mint(address to_, uint256 amount_) public {
        _mint(to_, amount_);
    }

    function burn(address to_, uint256 amount_) public {
        _burn(to_, amount_);
    }
}
