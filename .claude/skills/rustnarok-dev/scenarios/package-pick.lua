-- The package pick scenario's table, in SelectPackageItem_Cln's form: rathena's example box Select_Example1 (101060)
-- and the items its db/re/item_packages.yml gives for each group (set).
function LoadTable()
	AddTab(101060, 0, "Set A", 4)
	AddItem(101060, 0, 0, 28144, 1, 0, 0, false, 1, 4)
	AddTab(101060, 1, "Set B", 4)
	AddItem(101060, 1, 0, 640045, 1, 0, 0, false, 1, 5)
	AddItem(101060, 1, 1, 650039, 1, 0, 0, false, 2, 5)
	AddItem(101060, 1, 2, 700074, 1, 0, 0, false, 3, 5)
	AddItem(101060, 1, 3, 800033, 1, 0, 0, false, 4, 5)
	AddTab(101060, 2, "Set C", 5)
	AddItem(101060, 2, 0, 28144, 1, 3, 0, false, 1, 4)
	return true, "good"
end
