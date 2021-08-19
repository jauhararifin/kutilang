package core

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
)

func parseDate(dateStr string) (string, error) {
	parts := strings.Split(dateStr, "-")
	if len(parts) != 3 {
		return "", errors.New("invalid date format (yyyy-mm-dd)")
	}

	year, err := strconv.ParseUint(parts[0], 10, 32)
	if err != nil {
		return "", errors.New("invalid year")
	}

	if year < 1 || year > 9999 {
		return "", errors.New("invalid year")
	}

	month, err := strconv.ParseUint(parts[1], 10, 32)
	if err != nil {
		return "", errors.New("invalid month")
	}

	if month < 1 || month > 12 {
		return "", errors.New("invalid month")
	}

	date, err := strconv.ParseUint(parts[2], 10, 32)
	if err != nil {
		return "", errors.New("invalid date")
	}
	
	if date < 1 {
		return "", errors.New("invalid date")
	}

	if month == 1 || month == 3 || month == 5 || month == 7 || month == 8 || month == 10 || month == 12 {
		if date > 31 {
			return "", errors.New("invalid date")
		}
	} else if month == 2 {
		if isLeapYear(int(year)) {
			if date > 29 {
				return "", errors.New("invalid date")
			}
		} else {
			if date > 28 {
				return "", errors.New("invalid date")
			}
		}
	} else {
		if date > 30 {
			return "", errors.New("invalid date")
		}
	}

	return fmt.Sprintf("%04d-%02d-%02d", year, month, date), nil
}

func isLeapYear(year int) bool {
	if year % 4 == 0 {
		if year % 100 == 0 {
			return year % 400 == 0
		}
		return true
	}
	return false
}
