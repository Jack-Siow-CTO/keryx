# Console Skills UI is read-mostly for 1.0

Console 1.0 must list and view Skill packages via control-plane GET APIs and surface which skills a Run loaded. Authoring/editing packages in Console is optional (Should), not a release gate—trusted `skill_manage` / filesystem / agent paths remain valid writers. Skills Hub, registries, and full package IDE are out of 1.0. Keeps big-bang scope on operator browse and Run transparency without re-building a skill CMS.
