# Clean up yum cache
yum clean all

# Enable EPEL repository
yum -y install http://dl.fedoraproject.org/pub/epel/epel-release-latest-7.noarch.rpm
sed 's/enabled=.*/enabled=1/g' -i /etc/yum.repos.d/epel.repo

# Add Tarantool repository
rm -f /etc/yum.repos.d/*tarantool*.repo
tee /etc/yum.repos.d/tarantool_1_10.repo <<- EOF
[tarantool_1_10]
name=EnterpriseLinux-7 - Tarantool
baseurl=http://download.tarantool.org/tarantool/1.10/el/7/x86_64/
gpgkey=http://download.tarantool.org/tarantool/1.10/gpgkey
repo_gpgcheck=1
gpgcheck=0
enabled=1

[tarantool_1_10-source]
name=EnterpriseLinux-7 - Tarantool Sources
baseurl=http://download.tarantool.org/tarantool/1.10/el/7/SRPMS
gpgkey=http://download.tarantool.org/tarantool/1.10/gpgkey
repo_gpgcheck=1
gpgcheck=0
EOF

# Update metadata
yum makecache -y --disablerepo='*' --enablerepo='tarantool_1_10' --enablerepo='epel'
