#!/bin/bash
shopt -s expand_aliases

if [[ "$OSTYPE" == "darwin"* ]]; then
    # Mac OSX
	if ! command -v gsed &> /dev/null
	then
			echo "gsed could not be found"
			exit 1
	fi
	alias sed=gsed
elif [[ "$OSTYPE" == "freebsd"* ]]; then
	if ! command -v gsed &> /dev/null
	then
			echo "gsed could not be found"
			exit 1
	fi
	alias sed=gsed
fi

# takes the tag as an argument (e.g. v0.1.0)
if [ -n "$1" ]; then	
	# update the version
	msg="# managed by release.sh"
	
	sed "s/^version = .* $msg$/version = \"${1#v}\" $msg/" -i core/dtn7/Cargo.toml
	# update the version in examples/Cargo.toml for dtn7
	sed "s/^dtn7 = .* $msg$/dtn7 = { path = \"..\/core\/dtn7\", default-features = false, version = \"${1#v}\"} $msg/" -i examples/Cargo.toml

	# update the changelog
	git cliff --tag "$1" > CHANGELOG.md
	sleep 1
	cargo check
	git add -A && git commit -m "chore(release): prepare for $1"
	git show
	# generate a changelog for the tag message
	export TEMPLATE="\
	{% for group, commits in commits | group_by(attribute=\"group\") %}
	{{ group | upper_first }}\
	{% for commit in commits %}
		- {{ commit.message | upper_first }} ({{ commit.id | truncate(length=7, end=\"\") }})\
	{% endfor %}
	{% endfor %}"
	changelog=$(git cliff --unreleased --strip all)
	
	git tag -a "$1" -m "Release $1" -m "$changelog"
	read -p "Directly push to GitHub? " -n 1 -r
	echo    # (optional) move to a new line
	if [[ $REPLY =~ ^[Yy]$ ]]
	then
		git push
		sleep 1
		git push origin "$1"
	else
		echo "Don't forget to push tag to origin: "
		echo git push
		echo git push origin "$1"
	fi
else
    echo Changes since last release:
    git cliff --unreleased --strip all
    echo
    echo
	echo "warn: please provide a tag"
fi